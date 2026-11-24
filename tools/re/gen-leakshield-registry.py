"""Generate crates/nicecooling-protocol/registry/leakshield.toml and the report 3 table of docs/protocol/leakshield.md.

Usage: python3 -I tools/re/gen-leakshield-registry.py <repo>
The table replaces the text between the report3-table markers of the document.
Offsets count from the report ID byte. Ranges are in engineering units (raw * scale).
"""
import struct
import sys

REPO = sys.argv[1]
FIX = REPO + "/tests/fixtures/leakshield/fw1022/report3.bin"

E = lambda *pairs: ("enum", [c for c, _ in pairs], [n for _, n in pairs])
def names(codes, *names):
    assert len(codes) == len(names), (codes, names)
    return list(zip(codes, names))
L = lambda lo, hi: ("limit", lo, hi)
B = lambda m: ("bits", m)
T = ("type",)
LED_MODES = names([0, 1, 2, 3, 4, 5, 7, 8, 9, 10, 11, 12, 13, 14, 18, 19, 33],
    "unused", "fixed-color", "breathing", "rainbow", "blink-1", "blink-2", "sequential", "visor-1", "visor-2",
    "marquee-1", "marquee-2", "ambient", "bar", "candle", "switch", "rainbow-shift", "gradient")
RESERVOIRS = names([*range(12), 255], "undefined", "ultitube-100", "ultitube-150", "ultitube-200", "aqualis-100",
    "aqualis-150", "aqualis-450", "aqualis-880", "aquainlet-100", "aquainlet-150", "aquatube", "aquabox", "custom")
PUMPS = names([*range(9), 255], "undefined", "d5", "aquastream", "ddc-1t", "ddc-1t-plus", "ddc-325", "eheim-1046",
    "eheim-1048", "eheim-1250", "custom")
LOG_INTERVALS = names(range(8), "500ms", "1s", "5s", "10s", "30s", "1min", "5min", "10min")
UNITS = names(range(48), "none", "none-1", "none-2", "rpm", "percent", "fps", "ma", "a", "mv", "v", "degc", "degf",
    "l-per-h", "l-per-min", "gal-per-h", "gal-per-min", "hz", "khz", "mhz", "ghz", "mw", "w", "kw", "ms", "s", "min",
    "h", "days", "bit", "kbit", "mbit", "gbit", "mbar", "bar", "ratio", "mb", "gb", "tb", "bool", "g", "kg", "t", "deg",
    "humidity", "l", "conductivity", "mbar-per-min", "ml-per-h")
BRIGHTNESS = names(range(4), "maximum", "mid", "low", "off")
TEMPLATES = names([*range(14), 255], "logo", "chart", "values-1", "values-2", "values-4", "values-3a", "values-3b",
    "time", "bars-2", "bars-4a", "bars-4b", "value-and-bar", "value-and-time", "leakshield-state", "none")
RESPONSE_ACTIONS = names(range(4), "none", "signal-short", "signal-long", "key-sequence")
DEFAULT_MODES = names([1, 2, 3], "shield", "monitor", "release")
KEYS = names([0], "none") \
    + names(range(4, 30), *"abcdefghijklmnopqrstuvwxyz") \
    + names(range(30, 40), *"1234567890") \
    + names(range(40, 47), "enter", "escape", "backspace", "tab", "space", "minus", "equals") \
    + names(range(58, 70), *[f"f{n}" for n in range(1, 13)]) \
    + names([70, 72], "print-screen", "pause") \
    + names(range(74, 83), "home", "page-up", "delete", "end", "page-down", "right", "left", "down", "up") \
    + names(range(84, 89), "kp-divide", "kp-multiply", "kp-minus", "kp-plus", "kp-enter") \
    + names(range(89, 98), *[f"kp-{n}" for n in range(1, 10)]) + names([98], "kp-0") \
    + names(range(104, 116), *[f"f{n}" for n in range(13, 25)]) \
    + names([1064, 1066, 1067, 1070, 1071, 1072, 1135, 1181, 1182, 1205, 1226, 1233, 1234, 2001, 2002, 2004],
            "menu", "menu-up", "menu-down", "menu-escape", "menu-increase", "menu-decrease", "media-select",
            "next-track", "previous-track", "play-pause", "mute", "volume-up", "volume-down", "power", "sleep", "wake")

# id, offset, type, scale, unit, range, default, risk, description, extra
F = []
def f(id, off, typ, rng, risk, desc, scale=1, unit="", default=None, **kw):
    F.append(dict(id=id, offset=off, type=typ, range=rng, risk=risk, desc=desc, scale=scale, unit=unit, default=default, **kw))

f("signal_output_config", 3, "u8", T, "safety", "Setting of the signal output. The code does not define its values; aquasuite keeps the byte as read.")
f("fill_level_calibration", 4, "i16", L(-20, 20), "safety", "Correction of the fill level.", scale=0.01, unit="%")
f("reservoir_model", 6, "u8", E(*RESERVOIRS), "safety", "Reservoir preset: 0 undefined, 1-11 Aqua Computer reservoirs, 255 custom.")
f("reservoir_air_volume_full", 7, "i16", L(0, 32767), "safety", "Air volume of the reservoir at fill level 100 %.", unit="ml")
f("reservoir_air_volume_empty", 9, "i16", L(0, 32767), "safety", "Air volume of the reservoir at fill level 0 %.", unit="ml")
f("reservoir_total_volume", 11, "i16", L(0, 32767), "safety", "Total volume of the reservoir.", unit="ml")
f("pump_model", 13, "u8", E(*PUMPS), "safety", "Pump preset: 0 undefined, 1 D5, 2 aquastream, 3-5 DDC, 6-8 Eheim, 255 custom.")
f("pump_flags", 14, "u8", B(0x03), "safety", "Bit 0: custom pump. Bit 1: dual pump.")
f("pump_max_speed", 15, "i16", L(0, 32767), "safety", "Pump speed at full duty (\"max speed\").", unit="rpm", default=4800)
f("pump_max_pressure", 17, "i16", L(0, 3276.7), "safety", "Pump pressure at full speed (\"max pressure\").", scale=0.1, unit="mbar", default=365.0)
f("pump_start_speed", 19, "i16", L(0, 32767), "safety", "Pump speed that the device assumes without a Feed value (\"start speed\"). aquasuite keeps it at or below `pump_max_speed`.", unit="rpm", default=4800)
f("pump_start_flow", 21, "i16", L(0, 3276.7), "safety", "Flow that the device assumes without a Feed value (\"start flow\").", scale=0.1, unit="l/h", default=100.0)
f("sensor_log_interval", 23, "u8", E(*LOG_INTERVALS), "low", "Log interval of each device sensor: 0 0.5 s, 1 1 s, 2 5 s, 3 10 s, 4 30 s, 5 1 min, 6 5 min, 7 10 min.", count=20, stride=2)
f("sensor_unit", 24, "u8", E(*UNITS), "safety", "Unit code of each device sensor (unit code table). The Feed pump speed and flow use sensors 4 and 5, so the unit sets their scale.", count=20, stride=2)
f("display_page_time", 63, "u16", L(0, 6553.5), "low", "Time that each display page shows. 30 s and more: no automatic page change; aquasuite offers 1 to 29 s.", scale=0.1, unit="s")
f("display_brightness_active", 65, "u8", E(*BRIGHTNESS), "low", "0 maximum, 1 mid, 2 low, 3 off.")
f("display_brightness_idle", 66, "u8", E(*BRIGHTNESS), "low", "0 maximum, 1 mid, 2 low, 3 off.")
f("display_idle_delay", 67, "u16", T, "low", "Delay before the display goes idle. aquasuite always writes 2400. Unit not defined in the code.", default=2400)
f("clock_flags", 69, "u8", B(0x07), "safety", "Bit 0: daylight saving time on. Bit 1: 12 h format. Bit 2: daylight saving region US (clear: EU).")
f("clock_timezone", 70, "i16", L(-12, 14), "safety", "Time zone offset of the device local time.", scale=0.1, unit="h")
f("display_flags", 72, "u8", B(0x0F), "low", "Bit 0: rotate the display content by 180 degrees. Bit 1: display stays on in standby (clear: off after 10 minutes in standby). Bit 2: invert. Bit 3: automatic invert off.")
P = dict(count=10, stride=46)
f("page_template", 73, "u8", E(*TEMPLATES), "low", "Page layout: 0 logo, 1 chart, 2-6 values, 7 time, 8-10 bars, 11 value and bar, 12 value and time, 13 Leakshield state, 255 none.", **P)
f("page_options", 74, "u8", B(0x03), "low", "Bit 0: show the page in standby. Bit 1: hide the page in normal operation.", **P)
f("page_icons", 75, "u8[4]", T, "low", "Index into the icons of report 8, 255 none.", **P)
f("page_names", 79, "u8[8]", T, "low", "Index into the names of report 8, 255 none.", **P)
f("page_sources", 87, "u8[8]", T, "low", "Value source of each page slot: the sensor index times 4, 255 none. A value of 80 or more has no sensor.", **P)
f("page_precisions", 95, "u8[8]", T, "low", "Decimal places of each page slot.", **P)
f("page_values", 103, "i16[8]", T, "low", "Template parameters of each page slot.", **P)
f("standby_conditions", 533, "u8", B(0x0B), "safety", "Bit 0: standby without USB connection. Bit 1: standby on USB suspend. Bit 3: leave standby on alarm.")
f("alarm_start_delay", 534, "u8", T, "safety", "Alarm start timeout. aquasuite keeps the byte as read. Unit not defined in the code.")
f("alarm_enable", 535, "u16", B(0x0F), "safety", "Bit 0: fill level warning. Bit 1: fill level alarm. Bits 2-3: leak rate warning.")
f("alarm_buzzer", 537, "u16", B(0x0F), "safety", "Buzzer for the same bits as `alarm_enable`.")
f("fill_level_warning_threshold", 539, "i16", L(0, 100), "safety", "Fill level below which the warning starts.", scale=0.01, unit="%")
f("fill_level_alarm_threshold", 545, "i16", L(0, 100), "safety", "Fill level below which the alarm starts.", scale=0.01, unit="%")
f("leak_rate_warning_threshold", 551, "i16", L(50, 200), "safety", "Leak rate above which the warning starts.", unit="ml/h")
for name, off in (("fill_level_warning", 541), ("fill_level_alarm", 547), ("leak_rate_warning", 553),
                  ("leak_rate_alarm_1", 557), ("leak_rate_alarm_2", 561), ("system_alarm", 565)):
    f(f"{name}_response_action", off, "u8", E(*RESPONSE_ACTIONS), "safety", "Device response: 0 none, 1 power switch short, 2 power switch long, 3 USB key sequence.")
    f(f"{name}_response_modifiers", off + 1, "u8", B(0x0F), "safety", "Key modifiers of the USB key sequence: bit 0 ctrl, bit 1 shift, bit 2 alt, bit 3 windows.")
    f(f"{name}_response_key", off + 2, "u16", E(*KEYS), "safety", "Key code of the USB key sequence. Codes 4-115 follow the USB HID keyboard usages.")
f("loop_height", 569, "u8", L(0, 200), "safety", "Height of the loop.", unit="cm")
f("refill_duration_warning", 570, "u8", L(5, 240), "safety", "Pump duration of a refill above which a warning starts.", unit="s", default=5)
f("led_strip_brightness", 571, "u8", L(0, 100), "low", "Brightness of the LED strip.", scale=100 / 255, unit="%")
f("led_strip_flags", 572, "u16", B(0x06), "low", "Bit 1: strip off. Bit 2: strip on in standby.")
C = dict(count=2, stride=70)
f("led_controller_strip", 574, "u8", T, "low", "Strip of the LED controller.", **C)
f("led_controller_first_led", 575, "u8", T, "low", "First LED of the LED controller.", **C)
f("led_controller_led_count", 576, "u8", T, "low", "Number of LEDs of the LED controller.", **C)
f("led_controller_mode", 577, "u8", E(*LED_MODES), "low", "Effect: 0 unused, 1 fixed color, 2 breathing, 3 rainbow, 19 rainbow shift, 12 ambient, 4-5 blink, 7 sequential, 8-9 visor, 10-11 marquee, 14 candle, 18 switch, 13 bar, 33 gradient.", **C)
f("led_controller_flags", 578, "u16", T, "low", "Bit 14: binding 1 off. Bit 15: binding 2 off. Other bits are effect options.", **C)
f("led_controller_source", 580, "i16", T, "low", "Sensor that drives the effect.", **C)
f("led_controller_source_filter", 582, "u8[2]", T, "low", "Rise and fall filter of the source.", **C)
for k, off in ((1, 584), (2, 590)):
    f(f"led_controller_binding_{k}_x1", off, "i16", T, "low", "Lower source value of the binding.", **C)
    f(f"led_controller_binding_{k}_x2", off + 2, "i16", T, "low", "Upper source value of the binding.", **C)
    f(f"led_controller_binding_{k}_y1", off + 4, "u8", T, "low", "Output at the lower source value.", **C)
    f(f"led_controller_binding_{k}_y2", off + 5, "u8", T, "low", "Output at the upper source value.", **C)
f("led_controller_parameters", 596, "i16[12]", T, "low", "Effect parameters.", **C)
f("led_controller_color_hue", 620, "u16", L(0, 360), "low", "Hue of each of the 6 colors.", unit="deg", **C, inner_count=6, inner_stride=4)
f("led_controller_color_saturation", 622, "u8", T, "low", "Saturation of each of the 6 colors.", **C, inner_count=6, inner_stride=4)
f("led_controller_color_value", 623, "u8", T, "low", "Value (brightness) of each of the 6 colors.", **C, inner_count=6, inner_stride=4)
f("shield_flags", 714, "i16", B(0xFF), "safety", "Bit 1: time-controlled vacuum on. Bit 2: compensate the pressure equalisation membrane. Bit 3: adaptive pressure. Bit 4: default settings (clear: custom settings). Bit 5: standby pressure. Bit 6: permanent alarm buzzer. Bit 7: alarm buzzer off. Bit 0 is not defined in the code; aquasuite keeps it.")
f("vacuum_timer_start_hour", 716, "u8", L(0, 23), "safety", "Start of each of the 2 time-controlled vacuum windows.", unit="h", count=2, stride=2)
f("vacuum_timer_start_minute", 717, "u8", L(0, 59), "safety", "Start minute of each window.", unit="min", count=2, stride=2)
f("vacuum_timer_end_hour", 720, "u8", L(0, 23), "safety", "End of each window. aquasuite writes the start plus 1 min.", unit="h", count=2, stride=2)
f("vacuum_timer_end_minute", 721, "u8", L(0, 59), "safety", "End minute of each window.", unit="min", count=2, stride=2)
f("default_mode", 724, "u8", E(*DEFAULT_MODES), "safety", "Mode after power-up and after an Action: 1 SHIELD, 2 MONITOR, 3 RELEASE.")
for name, off in (("test_pressure", 725), ("deaeration_pressure", 727), ("refill_pressure", 729)):
    f(name, off, "i16", L(50, 450), "safety", f"Pressure of the Action ({name.split('_')[0]}).", scale=0.1, unit="mbar")
for mode, off in (("shield", 731), ("monitor", 739)):
    f(f"{mode}_pressure_low", off, "i16", L(30, 430), "safety", f"Lower pressure limit in {mode.upper()}.", scale=0.1, unit="mbar", default=100.0)
    f(f"{mode}_pressure_low_standby", off + 2, "i16", L(30, 430), "safety", f"Lower pressure limit in {mode.upper()} while the PC is in standby. aquasuite keeps it at or below the lower limit.", scale=0.1, unit="mbar", default=100.0)
    f(f"{mode}_pressure_working_range", off + 4, "i16", L(0, 420), "safety", "Setpoint minus lower limit.", scale=0.1, unit="mbar", default=50.0)
    f(f"{mode}_pressure_max_range", off + 6, "i16", L(0, 450), "safety", "Upper limit minus setpoint.", scale=0.1, unit="mbar", default=50.0)
f("device_profile", 747, "u8", L(1, 4), "safety", "Number of the device profile: 1-4, one fixed slot each. aquasuite writes the slot number before it stores the report of a slot. Only a Profile activation sets it.")

SIZES = {"u8": (1, "B"), "u16": (2, "H"), "i16": (2, "h")}

def base(t):
    b, _, n = t.partition("[")
    return b, int(n[:-1]) if n else 1

def values(raw, e):
    b, n = base(e["type"])
    sz, fmt = SIZES[b]
    out = []
    for i in range(e.get("count", 1)):
        for j in range(e.get("inner_count", 1)):
            o = e["offset"] + i * e.get("stride", 0) + j * e.get("inner_stride", 0)
            out += list(struct.unpack(">" + fmt * n, raw[o:o + sz * n]))
    return out

def in_range(v, e):
    r = e["range"]
    if r[0] == "enum":
        return v in r[1]
    if r[0] == "bits":
        return v & ~r[1] & 0xFFFF == 0
    if r[0] == "limit":
        x = v * e["scale"]
        return r[1] - 1e-9 <= x <= r[2] + 1e-9
    return True

raw = open(FIX, "rb").read()
assert len(raw) == 750
for e in F:
    vals = values(raw, e)
    e["fixture"] = vals
    ok = all(in_range(v, e) for v in vals)
    e["fixture_ok"] = ok
    if e["risk"] == "low" and e["range"][0] != "type" and ok:
        e["state"] = "plausible-read"
    else:
        e["state"] = "code-defined"
    if e["range"][0] == "limit" and not ok:
        print("OUT OF RANGE", e["id"], vals, file=sys.stderr)

def fmtnum(x):
    if isinstance(x, float):
        return repr(round(x, 6))
    return str(x)

lines = ['# LEAKSHIELD field registry. Source of truth for the report 3 fields, their risk class',
         '# and their verification state. docs/protocol/leakshield.md describes the format.',
         '', 'schema_version = 1', 'model = "LEAKSHIELD"', 'firmware = 1022', 'structure = 100', '',
         '[report3]', 'size = 750', 'structure_offset = 1', 'crc = { algorithm = "crc16_usb", first = 1, last = 747, offset = 748 }',
         '# Byte offsets that the device changes by itself. Empty: no byte differs between the fw1019 and',
         '# fw1022 dumps, and the code defines none. The Verification writes check it again.',
         'volatile_mask = []', '']
cmds = [("restart", 1, "1: restart into the bootloader, 0: plain restart (docs/re/firmware-update.md)"), ("save_settings", 2, "0"), ("factory_defaults", 3, "0"),
        ("show_page", 4, "page id << 16 | duration in 0.1 s"),
        ("display_on", 9, "0"), ("display_off", 10, "0"), ("reset_alarm", 11, "0"), ("set_mode", 12, "Mode or Action code"),
        ("cancel_action", 12, "0: stops the running Action"),
        ("reset_refill_history", 13, "0"), ("reset_event_log", 14, "0"), ("set_fill_mode", 15, "fill mode; values not defined in the code"),
        ("suppress_alarm", 16, "value / 25; aquasuite sends 90000 / 25 when the PC suspends"), ("usb_reset", 523, "0"), ("disable_low_volume_alarm", 14356, "65341"),
        ("reset_service_counter", 25000, "serial (report 1 bytes 3-6 as u32) XOR 0xFFFFFFFF"), ("test_mode", 25001, "129853 on, 958634 off"),
        ("set_clock", 37000, "seconds since 2009-01-01 00:00 UTC")]
lines += ['# Report 2 commands: output report 02, command u32 BE, parameter u32 BE, CRC-16/USB BE over the 8 bytes.',
          '# No command names the pump runtime (docs/protocol/leakshield.md, open questions).']
for name, code, par in cmds:
    lines += ['[[command]]', f'id = "{name}"', f'code = {code}', f'parameter = "{par}"', '']
for e in F:
    lines.append('[[field]]')
    lines.append(f'id = "{e["id"]}"')
    lines.append('report = 3')
    lines.append(f'offset = {e["offset"]}')
    lines.append(f'type = "{e["type"]}"')
    for k in ("count", "stride", "inner_count", "inner_stride"):
        if k in e:
            lines.append(f'{k} = {e[k]}')
    if e["scale"] != 1:
        lines.append(f'scale = {fmtnum(float(e["scale"]))}')
    if e["unit"]:
        lines.append(f'unit = "{e["unit"]}"')
    r = e["range"]
    if r[0] == "limit":
        lines.append(f'min = {fmtnum(r[1])}')
        lines.append(f'max = {fmtnum(r[2])}')
    elif r[0] == "enum":
        lines.append('values = [' + ", ".join(map(str, r[1])) + ']')
        lines.append('names = [' + ", ".join(f'"{n}"' for n in r[2]) + ']')
    elif r[0] == "bits":
        lines.append(f'bits = 0x{r[1]:04X}')
    if e["default"] is not None:
        lines.append(f'default = {fmtnum(e["default"])}')
    lines.append(f'risk_class = "{e["risk"]}"')
    lines.append(f'verification_state = "{e["state"]}"')
    lines.append('')
open(REPO + "/crates/nicecooling-protocol/registry/leakshield.toml", "w").write("\n".join(lines))

# Doc table (no risk class, no verification state: those live in the registry).
def rng(e):
    r = e["range"]
    u = (" " + e["unit"]) if e["unit"] else ""
    if r[0] == "limit":
        return f"{fmtnum(r[1])} to {fmtnum(r[2])}{u}"
    if r[0] == "enum":
        v = r[1]
        runs, a = [], v[0]
        for i in range(1, len(v) + 1):
            if i == len(v) or v[i] != v[i - 1] + 1:
                runs.append(str(a) if a == v[i - 1] else f"{a}-{v[i - 1]}")
                if i < len(v):
                    a = v[i]
        txt = ", ".join(runs)
        return "codes " + (txt if len(runs) <= 8 else f"{v[0]}-{v[-1]}, {len(v)} codes (list in the registry)")
    if r[0] == "bits":
        return f"mask 0x{r[1]:X}"
    return "type only"
T_ = ["| Field id | Offset | Type | Repeat | Scale | Range | aquasuite default | Fixture fw1022 | Meaning |", "|---|---|---|---|---|---|---|---|---|"]
for e in F:
    rep = ""
    if "count" in e:
        rep = f'{e["count"]} x {e["stride"]}'
        if "inner_count" in e:
            rep += f', {e["inner_count"]} x {e["inner_stride"]}'
    vals = e["fixture"]
    fv = str(vals[0]) if len(vals) == 1 else (str(vals[:6])[:-1] + ", ...]" if len(vals) > 6 else str(vals))
    sc = "" if e["scale"] == 1 else ("1/2.55" if abs(e["scale"] - 100 / 255) < 1e-9 else fmtnum(float(e["scale"])))
    T_.append(f'| `{e["id"]}` | {e["offset"]} | {e["type"]} | {rep} | {sc} | {rng(e)} | {"" if e["default"] is None else fmtnum(e["default"])} | {fv} | {e["desc"]} |')
doc = REPO + "/docs/protocol/leakshield.md"
d = open(doc).read()
a = d.index("<!-- report3-table:start")
a = d.index("\n", a) + 1
b = d.index("<!-- report3-table:end -->")
open(doc, "w").write(d[:a] + "\n".join(T_) + "\n" + d[b:])
n_s = sum(e["risk"] == "safety" for e in F)
print("fields", len(F), "safety", n_s, "low", len(F) - n_s, "plausible", sum(e["state"] == "plausible-read" for e in F), file=sys.stderr)
for e in F:
    if e["risk"] == "low" and e["state"] != "plausible-read":
        print("low code-defined:", e["id"], e["range"][0], e["fixture_ok"], file=sys.stderr)
