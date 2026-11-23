#!/usr/bin/env python3
"""Tests of usbmon-summary.py on the synthetic capture of tests/fixtures/usbmon-sample/.

Run: python3 -I tools/re/test-usbmon-summary.py
Each mutation adds or changes packets of the sample and expects the summary of the sample, or the
change that the mutation must cause. The tests read files in a temporary directory only.
"""

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
SAMPLE = ROOT / "tests/fixtures/usbmon-sample"

spec = importlib.util.spec_from_file_location("usbmon_summary", ROOT / "tools/re/usbmon-summary.py")
tool = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tool)
Pairs = tool.Pairs


def obj(**kw):
    return Pairs(kw.items())


def packet(time, dev, kind, xfer, endpoint, status, **layers):
    usb = obj(**{
        "usb.urb_type": f"'{kind}'",
        "usb.transfer_type": xfer,
        "usb.endpoint_address": endpoint,
        "usb.device_address": str(dev),
        "usb.bus_id": "1",
        "usb.urb_status": str(status),
    })
    frame = obj(**{"frame.time_relative": f"{time:.9f}"})
    parts = [("frame", frame), ("usb", usb)] + [(k, obj(**v)) for k, v in layers.items()]
    return obj(_source=obj(layers=Pairs(parts)))


def device_descriptor_request(time, dev):
    return packet(time, dev, "S", "0x02", "0x80", -115, **{"Setup Data": {
        "usb.bmRequestType": "0x80", "usb.setup.bRequest": "6", "usb.DescriptorIndex": "0x00",
        "usb.bDescriptorType": "0x01", "usb.setup.wLength": "64"}})


def device_descriptor_reply(time, dev):
    return packet(time, dev, "C", "0x02", "0x80", 0, **{"DEVICE DESCRIPTOR": {
        "usb.bDescriptorType": "0x01", "usb.idVendor": "0x0c70", "usb.idProduct": "0xf014",
        "usb.bcdDevice": "0x0200"}})


def dump(node):
    if isinstance(node, Pairs):
        return "{" + ",".join(f"{json.dumps(k)}:{dump(v)}" for k, v in node) + "}"
    if isinstance(node, list):
        return "[" + ",".join(dump(v) for v in node) + "]"
    return json.dumps(node)


def time_of(p):
    layers = dict(dict(p)["_source"])["layers"]
    return float(dict(dict(layers)["frame"])["frame.time_relative"])


def summary(packets):
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "capture.json"
        path.write_text(dump(sorted(packets, key=time_of)), encoding="utf-8")
        return "\n".join(tool.summarize(*tool.read_packets(str(path), None))) + "\n"


def sample():
    with open(SAMPLE / "capture.json", encoding="utf-8") as handle:
        return json.load(handle, object_pairs_hook=Pairs)


def without_set_reports(text):
    """An added SET_REPORT shows in the list. The other lines must stay."""
    return [line for line in text.splitlines() if not line.startswith("SET_REPORT")]


EXPECTED = (SAMPLE / "expected.txt").read_text(encoding="utf-8")


class Summary(unittest.TestCase):
    def test_sample_matches_expected(self):
        self.assertEqual(summary(sample()), EXPECTED)

    def test_descriptor_at_address_0_is_not_the_bootloader(self):
        # The host reads the device descriptor at address 0 before SET_ADDRESS.
        packets = sample() + [device_descriptor_request(0.340, 0), device_descriptor_reply(0.341, 0)]
        self.assertEqual(summary(packets), EXPECTED)

    def test_command_after_the_return_is_not_the_leave_command(self):
        packets = sample() + [
            packet(3.5, 7, "S", "0x02", "0x00", -115, **{"Setup Data": {
                "usb.bmRequestType": "0x21", "usb.setup.bRequest": "9", "usb.setup.wValue": "0x0202",
                "usb.setup.wIndex": "1", "usb.setup.wLength": "11",
                "usb.data_fragment": "02:00:00:00:0c:00:00:00:01:00:00"}}),
            packet(3.501, 7, "C", "0x02", "0x00", 0),
        ]
        self.assertEqual(without_set_reports(summary(packets)), without_set_reports(EXPECTED))

    def test_restart_0_before_the_bootloader_is_not_the_leave_command(self):
        early = packet(0.050, 5, "S", "0x02", "0x00", -115, **{"Setup Data": {
            "usb.bmRequestType": "0x21", "usb.setup.bRequest": "9", "usb.setup.wValue": "0x0202",
            "usb.setup.wIndex": "1", "usb.setup.wLength": "11",
            "usb.data_fragment": "02:00:00:00:01:00:00:00:00:34:82"}})
        done = packet(0.051, 5, "C", "0x02", "0x00", 0)
        self.assertEqual(
            without_set_reports(summary(sample() + [early, done])), without_set_reports(EXPECTED)
        )

    def test_request_without_descriptor_index_does_not_crash(self):
        packets = sample()
        for p in packets:
            layers = dict(dict(p)["_source"])["layers"]
            setup = dict(layers).get("Setup Data")
            if setup is not None and any(k == "usb.DescriptorIndex" for k, _ in setup):
                setup[:] = [(k, v) for k, v in setup if k != "usb.DescriptorIndex"]
        self.assertEqual(summary(packets), EXPECTED)

    def test_precedence_of_the_descriptor_value(self):
        self.assertEqual(tool.setup_of({"usb.bDescriptorType": "0x01"})[1], 0x0100)
        self.assertEqual(tool.setup_of({"usb.bDescriptorType": "0x02", "usb.DescriptorIndex": "0x03"})[1], 0x0203)
        self.assertEqual(tool.setup_of({"usbhid.descriptor.hid.bDescriptorType": "0x22"})[1], 0x2200)

    def test_report_1_length_is_the_urb_length(self):
        self.assertIn("bootloader report 1: URB length 64 bytes", EXPECTED)
        bytes_13_14 = EXPECTED.splitlines()[-1].split(": ", 2)[2].split()[13:15]
        self.assertEqual(bytes_13_14, ["00", "01"])


if __name__ == "__main__":
    unittest.main()
