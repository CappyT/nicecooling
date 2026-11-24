"""Generate docs/hardware-checks/verification-writes.md from the registry. Usage: python3 -I tools/re/gen-verification-writes.py <repo>"""
import struct, sys, tomllib
R = sys.argv[1]
reg = tomllib.load(open(R + "/crates/nicecooling-protocol/registry/leakshield.toml", "rb"))
raw = open(R + "/tests/fixtures/leakshield/fw1022/report3.bin", "rb").read()
SZ = {"u8": (1, "B"), "u16": (2, "H"), "i16": (2, "h")}

def first(e):
    b = e["type"].split("[")[0]
    n, f = SZ[b]
    return struct.unpack(">" + f, raw[e["offset"]:e["offset"] + n])[0]

def step(e):
    i = e["id"]
    if i.endswith("_response_action"):
        return "0 (none) when the current value is not 0, else 3 (USB key sequence). Never 1 or 2 (power switch)"
    if i == "default_mode":
        return "2 (MONITOR) when the current value is not 2, else 1 (SHIELD)"
    if "values" in e:
        return "the next allowed code after the current value"
    if "bits" in e:
        return "the current value with one defined bit toggled"
    s = e.get("scale", 1)
    return f"the current value plus or minus one raw step ({s:g} {e.get('unit', '')})".replace(" )", ")")

def extra(i):
    if i.startswith(("reservoir_", "fill_level_calibration")):
        return "Fill level stable for 5 min. Do not refill or drain during the check."
    if i.startswith("pump_"):
        return "Pump runs at its normal duty. No Action runs."
    if i.endswith(("_response_action", "_response_modifiers", "_response_key")) or i.startswith(("alarm_", "fill_level_", "leak_rate_", "daily_pump_time")):
        return "No Warning or Alarm is active: `alarm_bits` and `alarm_bits_latched` read 0."
    if i.startswith(("shield_pressure", "monitor_pressure")):
        return "Pressure stable within 2 mbar of the setpoint for 5 min. The written value keeps lower limit < setpoint < upper limit."
    if i.startswith(("test_", "deaeration_", "refill_pressure")):
        return "No Action runs, and none starts during the check."
    if i.startswith(("vacuum_timer", "clock_", "shield_flags")):
        return "No time-controlled vacuum window starts within 15 min of the check, also with the test value."
    if i == "default_mode":
        return "The replug step powers the device up in the Mode of the test value. Set the Mode back after the restore."
    if i == "standby_conditions":
        return "The PC does not suspend during the check."
    if i == "loop_height":
        return "Pressure stable within 2 mbar of the setpoint for 5 min."
    return ""

safety = [e for e in reg["field"] if e["risk_class"] == "safety"]
out = ["# Verification writes", "",
       "This procedure runs one Verification write for each Safety field of `crates/nicecooling-protocol/registry/leakshield.toml` with `tools/nicecooling-verify` (task 5.12). Task 5.13 runs it. A Safety field is writable only after its Verification write passes (design D10). `signal_output_config` and `alarm_start_delay` have no safe test value: they have no Verification write and stay read-only.", "",
       "## Base safe state", "",
       "Every check starts from this state. A field section adds its own conditions.", "",
       "1. The daemon is stopped, so the tool owns the device. The out-of-tree driver is not loaded.",
       "2. The loop is filled and leak-free. The pump runs at its normal duty. The PC is idle: no load that needs full cooling.",
       "3. The Leakshield is in Mode SHIELD. No Action runs. `alarm_bits` reads 0.",
       "4. A person watches the loop for the whole check and can stop the pump.",
       "5. The test value stays inside the registry range. For a field with repeats, the check writes copy 0 only.",
       "6. The tool takes a backup first and restores it at the end. A failed check also ends with the restore.", "",
       "## Result", "",
       "- Pass: the read-back differs from the backup only in the field, the CRC and the volatile mask, the value persists after the USB replug, and the restore brings back the backup bytes. Registry change: `verification_state` from `code-defined` to `verified-write`.",
       "- Fail: any other result. Registry change: `verification_state` from `code-defined` to `failed`. Record the tool output.", "",
       "Record each result in the section of the field as `Result: <date>, firmware 1022, pass|fail, <note>`.", "",
       "## Fields", ""]
for e in safety:
    rep = f", copy 0 of {e['count']}" if "count" in e else ""
    out += [f"### {e['id']}", "",
            f"- Offset {e['offset']}, {e['type']}{rep}. Value in the fw1022 dump: {first(e)}.",
            f"- Test value: {step(e)}."]
    x = extra(e["id"])
    if e["id"] in ("signal_output_config", "alarm_start_delay"):
        x = "The code does not define the meaning of the values, so no test value is safe. The field has no Verification write and stays read-only (owner decision, gate 2.14). Expected registry change: none, the field stays `code-defined`."
        out += [f"- Safe loop state: base. {x}", "- Result: none, read-only", ""]
        continue
    out += [f"- Safe loop state: base." + (f" {x}" if x else ""),
            "- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.",
            "- Result: open", ""]
open(R + "/docs/hardware-checks/verification-writes.md", "w").write("\n".join(out))
print(len(safety))
