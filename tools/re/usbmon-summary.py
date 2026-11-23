#!/usr/bin/env python3
"""Summarize a usbmon capture of a Leakshield firmware update (docs/hardware-checks/firmware-update-capture.md).

Input: the output of `tshark -r <capture> -T json`. The tool reads that one file. It opens no device.

Fields that it reads: usb.bus_id, usb.device_address, usb.urb_type, usb.urb_status,
usb.setup.bRequest, usb.setup.wValue, usb.setup.wLength, usb.data_fragment and frame.time_relative.
It reads usb.urb_len for the length of an interrupt report.
tshark 4.6 needs more for a real capture. A request of a HID interface has the fields usbhid.setup.*
and usbhid.descriptor.hid.* in place of usb.setup.*. A GET_DESCRIPTOR request of the device or the
configuration has usb.bDescriptorType and usb.DescriptorIndex in place of usb.setup.wValue. usb.bmRequestType, usb.transfer_type and
usb.endpoint_address tell SET_REPORT from SET_CONFIGURATION and an interrupt transfer from a
control transfer. usb.control.Response, usb.capdata and usbhid.data hold the data of a completed control IN or
interrupt IN transfer. tshark decodes a device descriptor (usb.idVendor, usb.idProduct,
usb.bcdDevice), a configuration descriptor (usb.bInterfaceNumber, usb.bNumEndpoints,
usb.bInterfaceClass) and a HID report descriptor (usbhid.item.* and one key per item), so the tool
reads those trees, in document order.

A Leakshield is a device that sends a device descriptor with 0c70:f014 at a nonzero address, or that receives a report 2
command or a feature report 32. The URB status maps 0 to ACK, -32 to STALL, and -2 or -104 to
timeout. The length of feature report 32 comes from the HID report descriptor of the bootloader.
"""

import argparse
import json
import sys

VENDOR_ID = 0x0C70
PRODUCT_ID = 0xF014
SET_REPORT = 9
GET_DESCRIPTOR = 6
DESC_DEVICE, DESC_CONFIG, DESC_REPORT = 1, 2, 0x22
CONTROL, INTERRUPT = 2, 1
REPORT_TYPES = {1: "input", 2: "output", 3: "feature"}
DATA_FIELDS = ("usb.data_fragment", "usb.control.Response", "usb.capdata", "usbhid.data")


class Pairs(list):
    """A JSON object as its (key, value) pairs: tshark repeats a key inside one object."""


def leaves(node, out=None):
    """The (key, value) pairs of a packet in document order. A key of a sub-tree has the value None."""
    out = [] if out is None else out
    for key, value in node:
        if isinstance(value, Pairs):
            out.append((key, None))
            leaves(value, out)
        else:
            out.append((key, value))
    return out


def number(text, default=None):
    if text is None:
        return default
    return int(str(text), 0)


def urb_type(text):
    text = str(text).strip("'")
    return chr(int(text, 0)) if text.startswith("0x") else text


def first_values(pairs):
    flat = {}
    for key, value in pairs:
        if value is not None:
            flat.setdefault(key, value)
    return flat


def setup_of(flat):
    """(bRequest, wValue, wIndex, wLength). tshark names a HID request usbhid.* once it knows the interface."""
    def pick(*names):
        return next((number(flat[n]) for n in names if n in flat), None)

    request = pick("usb.setup.bRequest", "usbhid.setup.bRequest")
    value = pick("usb.setup.wValue", "usbhid.setup.wValue")
    hid_type = pick("usbhid.descriptor.hid.bDescriptorType")
    if request is None and hid_type is not None:
        request = GET_DESCRIPTOR
        value = hid_type << 8 | (pick("usbhid.descriptor.hid.bDescriptorIndex") or 0)
    descriptor_type = pick("usb.bDescriptorType")
    if value is None and descriptor_type is not None:
        value = descriptor_type << 8 | (pick("usb.DescriptorIndex") or 0)
    return (
        request,
        value or 0,
        pick("usb.setup.wIndex", "usbhid.setup.wIndex", "usbhid.descriptor.hid.wInterfaceNumber") or 0,
        pick("usb.setup.wLength", "usbhid.setup.wLength", "usbhid.descriptor.hid.wDescriptorLength") or 0,
    )


def data_of(flat):
    for name in DATA_FIELDS:
        if flat.get(name):
            return bytes.fromhex(flat[name].replace(":", ""))
    return b""


def status_name(status):
    if status == 0:
        return "ACK"
    if status == -32:
        return "STALL"
    if status in (-2, -104):
        return "timeout"
    return f"status {status}"


def hexbytes(data):
    return " ".join(f"{b:02x}" for b in data)


def read_packets(path, bus_filter):
    """Pair each control submit with its completion and collect the interrupt transfers."""
    with open(path, encoding="utf-8") as handle:
        packets = json.load(handle, object_pairs_hook=Pairs)
    pending = {}
    controls, interrupts = [], []
    for packet in packets:
        pairs = leaves(packet)
        flat = first_values(pairs)
        bus, dev = number(flat.get("usb.bus_id")), number(flat.get("usb.device_address"))
        if bus is None or dev is None or (bus_filter is not None and bus != bus_filter):
            continue
        kind, xfer = urb_type(flat.get("usb.urb_type")), number(flat.get("usb.transfer_type"))
        endpoint = number(flat.get("usb.endpoint_address"), 0)
        time = float(flat["frame.time_relative"])
        data = data_of(flat)
        key = (bus, dev)
        if xfer == CONTROL and kind == "S":
            request, value, index, length = setup_of(flat)
            pending.setdefault(key, []).append(
                {
                    "key": key,
                    "time": time,
                    "bm": number(flat.get("usb.bmRequestType")),
                    "request": request,
                    "value": value,
                    "index": index,
                    "length": length,
                    "out": data,
                }
            )
        elif xfer == CONTROL and kind == "C" and pending.get(key):
            transfer = pending[key].pop(0)
            transfer["status"] = number(flat.get("usb.urb_status"), 0)
            transfer["done"] = time
            transfer["in"] = data
            transfer["leaves"] = pairs
            controls.append(transfer)
        elif xfer == INTERRUPT and data:
            incoming = kind == "C" and endpoint & 0x80
            outgoing = kind == "S" and not endpoint & 0x80
            if incoming or outgoing:
                interrupts.append(
                    {
                        "key": key,
                        "time": time,
                        "in": bool(incoming),
                        "data": data,
                        "urb_len": number(flat.get("usb.urb_len"), len(data)),
                    }
                )
    return controls, interrupts


def is_set_report(transfer):
    class_request = transfer["bm"] is None or transfer["bm"] & 0x60 == 0x20
    return class_request and transfer["request"] == SET_REPORT


def descriptor_request(transfer, kind):
    return (
        transfer["request"] == GET_DESCRIPTOR
        and transfer["value"] >> 8 == kind
        and (transfer["bm"] is None or transfer["bm"] & 0x60 == 0)
    )


def device_descriptors(controls):
    """(time, key) of each completed device descriptor of a Leakshield at a nonzero address.

    The host reads the first 8 or 18 bytes at address 0, before SET_ADDRESS. That read does not
    name the address that the device keeps.
    """
    found = []
    for t in controls:
        flat = first_values(t["leaves"])
        if t["key"][1] != 0 and descriptor_request(t, DESC_DEVICE) and "usb.idVendor" in flat:
            if (number(flat["usb.idVendor"]), number(flat.get("usb.idProduct"))) == (VENDOR_ID, PRODUCT_ID):
                found.append((t["done"], t["key"]))
    return found


def commands(controls, interrupts):
    """(time, key, command, parameter) of each report 2 that the host sends."""
    found = []
    for t in controls:
        d = t["out"]
        if is_set_report(t) and t["value"] == 0x0202 and len(d) >= 9 and d[0] == 2:
            found.append((t["time"], t["key"], int.from_bytes(d[1:5], "big"), int.from_bytes(d[5:9], "big")))
    for i in interrupts:
        d = i["data"]
        if not i["in"] and len(d) >= 9 and d[0] == 2:
            found.append((i["time"], i["key"], int.from_bytes(d[1:5], "big"), int.from_bytes(d[5:9], "big")))
    return sorted(found)


def feature_bits_tree(pairs):
    """Bits of the Feature items of each report ID, from the items that tshark decoded."""
    report_id, size, count, bits = 0, 0, 0, {}
    for key, value in pairs:
        if key == "usbhid.item.global.report_id":
            report_id = number(value)
        elif key == "usbhid.item.global.report_size":
            size = number(value)
        elif key == "usbhid.item.global.report_count":
            count = number(value)
        elif key.startswith("Feature"):
            bits[report_id] = bits.get(report_id, 0) + size * count
    return bits


def feature_bits(descriptor):
    """Bits of the Feature items of each report ID, from the raw bytes of the descriptor."""
    i, size, count, report_id, bits = 0, 0, 0, 0, {}
    while i < len(descriptor):
        prefix = descriptor[i]
        if prefix == 0xFE:
            i += 3 + (descriptor[i + 1] if i + 1 < len(descriptor) else 0)
            continue
        length = (0, 1, 2, 4)[prefix & 3]
        value = int.from_bytes(descriptor[i + 1 : i + 1 + length], "little")
        item = prefix & 0xFC
        if item == 0x84:
            report_id = value
        elif item == 0x74:
            size = value
        elif item == 0x94:
            count = value
        elif item == 0xB0:
            bits[report_id] = bits.get(report_id, 0) + size * count
        i += 1 + length
    return bits


def interfaces(pairs):
    """(number, endpoints, class) of each interface descriptor, in document order."""
    found = []
    for key, value in pairs:
        if key == "usb.bInterfaceNumber":
            found.append([number(value), None, None])
        elif found and key == "usb.bNumEndpoints":
            found[-1][1] = number(value)
        elif found and key == "usb.bInterfaceClass":
            found[-1][2] = number(value)
    return found


def summarize(controls, interrupts):
    lines = []
    descriptors = device_descriptors(controls)
    cmds = commands(controls, interrupts)
    set_reports = [t for t in controls if is_set_report(t)]
    ours = {key for _, key in descriptors} | {key for _, key, _, _ in cmds}
    ours |= {t["key"] for t in set_reports if t["value"] == 0x0320 and t["out"]}

    enter = next((c for c in cmds if c[2] == 1 and c[3] == 1), None)
    boot = None
    if enter:
        boot = next((d for d in descriptors if d[0] > enter[0]), None)
    boot_key = boot[1] if boot else None
    leave = None
    if boot:
        restarts = [c for c in cmds if c[2] == 1 and c[3] == 0 and c[0] > boot[0]]
        leave = restarts[-1] if restarts else None
    app = next((d for d in descriptors if leave and d[0] > leave[0]), None)

    report_length = "not observed"
    for t in controls:
        if boot_key and t["key"] == boot_key and descriptor_request(t, DESC_REPORT):
            tree = feature_bits_tree(t["leaves"]).get(0x20)
            bits = tree if tree is not None else feature_bits(t["in"]).get(0x20)
            if bits is not None:
                report_length = (
                    f"{(bits + 7) // 8 + 1} bytes with the report ID "
                    f"({bits} bits, interface {t['index']})"
                )
                break
    lines.append(f"feature report 32 length: {report_length}")

    found = []
    for t in controls:
        if boot_key and t["key"] == boot_key and descriptor_request(t, DESC_CONFIG):
            if len(interfaces(t["leaves"])) > len(found):
                found = interfaces(t["leaves"])
    if found:
        text = ", ".join(f"{n} (class 0x{c:02x}, {e} endpoints)" for n, e, c in found)
        lines.append(f"bootloader interfaces: {len(found)}: {text}")
    else:
        lines.append("bootloader interfaces: not observed")

    for t in set_reports:
        if t["key"] in ours:
            kind = REPORT_TYPES.get(t["value"] >> 8, f"type {t['value'] >> 8}")
            lines.append(
                f"SET_REPORT dev {t['key'][1]} {kind} id 0x{t['value'] & 0xFF:02x} "
                f"wLength {t['length']} {status_name(t['status'])} t={t['time']:.3f}"
            )

    first = next((t for t in set_reports if t["key"] in ours and t["value"] == 0x0320 and t["out"]), None)
    lines.append("first chunk, first 32 bytes: " + (hexbytes(first["out"][:32]) if first else "not observed"))

    for label, start, end in (("enter", enter, boot), ("leave", leave, app)):
        delay = f"{end[0] - start[0]:.3f} s" if start and end else "not observed"
        lines.append(f"time to {label} the bootloader: {delay}")

    seen = {}
    for i in interrupts:
        in_boot = boot and i["key"] == boot_key and boot[0] <= i["time"] and (not leave or i["time"] <= leave[0])
        if i["in"] and in_boot and i["data"][0] == 1:
            seen.setdefault((i["data"], i["urb_len"]), [i["time"], 0])[1] += 1
    for (data, urb_len), (time, count) in seen.items():
        lines.append(
            f"bootloader report 1: URB length {urb_len} bytes, count {count}, first t={time:.3f}: "
            f"{hexbytes(data[:64])}"
        )
    if not seen:
        lines.append("bootloader report 1: not observed")
    return lines


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("capture", help="the output of `tshark -r <capture> -T json`")
    parser.add_argument("--bus", type=int, help="read only this USB bus")
    args = parser.parse_args()
    controls, interrupts = read_packets(args.capture, args.bus)
    print("\n".join(summarize(controls, interrupts)))


if __name__ == "__main__":
    sys.exit(main())
