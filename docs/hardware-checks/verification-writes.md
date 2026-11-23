# Verification writes

This procedure runs one Verification write for each Safety field of `crates/nicecooling-protocol/registry/leakshield.toml` with `tools/nicecooling-verify` (task 5.12). Task 5.13 runs it. A Safety field is writable only after its Verification write passes (design D10). `signal_output_config` and `alarm_start_delay` have no safe test value: they have no Verification write and stay read-only.

## Base safe state

Every check starts from this state. A field section adds its own conditions.

1. The daemon is stopped, so the tool owns the device. The out-of-tree driver is not loaded.
2. The loop is filled and leak-free. The pump runs at its normal duty. The PC is idle: no load that needs full cooling.
3. The Leakshield is in Mode SHIELD. No Action runs. `alarm_bits` reads 0.
4. A person watches the loop for the whole check and can stop the pump.
5. The test value stays inside the registry range. For a field with repeats, the check writes copy 0 only.
6. The tool takes a backup first and restores it at the end. A failed check also ends with the restore.

## Result

- Pass: the read-back differs from the backup only in the field, the CRC and the volatile mask, the value persists after the USB replug, and the restore brings back the backup bytes. Registry change: `verification_state` from `code-defined` to `verified-write`.
- Fail: any other result. Registry change: `verification_state` from `code-defined` to `failed`. Record the tool output.

Record each result in the section of the field as `Result: <date>, firmware 1022, pass|fail, <note>`.

## Fields

### signal_output_config

- Offset 3, u8. Value in the fw1022 dump: 0.
- Test value: the current value plus or minus one raw step (1).
- Safe loop state: base. The code does not define the meaning of the values, so no test value is safe. The field has no Verification write and stays read-only (owner decision, gate 2.14). Expected registry change: none, the field stays `code-defined`.
- Result: none, read-only

### fill_level_calibration

- Offset 4, i16. Value in the fw1022 dump: 0.
- Test value: the current value plus or minus one raw step (0.01 %).
- Safe loop state: base. Fill level stable for 5 min. Do not refill or drain during the check.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### reservoir_model

- Offset 6, u8. Value in the fw1022 dump: 255.
- Test value: the next allowed code after the current value.
- Safe loop state: base. Fill level stable for 5 min. Do not refill or drain during the check.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### reservoir_air_volume_full

- Offset 7, i16. Value in the fw1022 dump: 65.
- Test value: the current value plus or minus one raw step (1 ml).
- Safe loop state: base. Fill level stable for 5 min. Do not refill or drain during the check.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### reservoir_air_volume_empty

- Offset 9, i16. Value in the fw1022 dump: 260.
- Test value: the current value plus or minus one raw step (1 ml).
- Safe loop state: base. Fill level stable for 5 min. Do not refill or drain during the check.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### reservoir_total_volume

- Offset 11, i16. Value in the fw1022 dump: 270.
- Test value: the current value plus or minus one raw step (1 ml).
- Safe loop state: base. Fill level stable for 5 min. Do not refill or drain during the check.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### pump_model

- Offset 13, u8. Value in the fw1022 dump: 1.
- Test value: the next allowed code after the current value.
- Safe loop state: base. Pump runs at its normal duty. No Action runs.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### pump_flags

- Offset 14, u8. Value in the fw1022 dump: 0.
- Test value: the current value with one defined bit toggled.
- Safe loop state: base. Pump runs at its normal duty. No Action runs.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### pump_max_speed

- Offset 15, i16. Value in the fw1022 dump: 4800.
- Test value: the current value plus or minus one raw step (1 rpm).
- Safe loop state: base. Pump runs at its normal duty. No Action runs.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### pump_max_pressure

- Offset 17, i16. Value in the fw1022 dump: 3750.
- Test value: the current value plus or minus one raw step (0.1 mbar).
- Safe loop state: base. Pump runs at its normal duty. No Action runs.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### pump_start_speed

- Offset 19, i16. Value in the fw1022 dump: 2500.
- Test value: the current value plus or minus one raw step (1 rpm).
- Safe loop state: base. Pump runs at its normal duty. No Action runs.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### pump_start_flow

- Offset 21, i16. Value in the fw1022 dump: 1300.
- Test value: the current value plus or minus one raw step (0.1 l/h).
- Safe loop state: base. Pump runs at its normal duty. No Action runs.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### clock_flags

- Offset 69, u8. Value in the fw1022 dump: 1.
- Test value: the current value with one defined bit toggled.
- Safe loop state: base. No time-controlled vacuum window starts within 15 min of the check, also with the test value.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### clock_timezone

- Offset 70, i16. Value in the fw1022 dump: 10.
- Test value: the current value plus or minus one raw step (0.1 h).
- Safe loop state: base. No time-controlled vacuum window starts within 15 min of the check, also with the test value.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### standby_conditions

- Offset 533, u8. Value in the fw1022 dump: 11.
- Test value: the current value with one defined bit toggled.
- Safe loop state: base. The PC does not suspend during the check.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### alarm_start_delay

- Offset 534, u8. Value in the fw1022 dump: 5.
- Test value: the current value plus or minus one raw step (1).
- Safe loop state: base. The code does not define the meaning of the values, so no test value is safe. The field has no Verification write and stays read-only (owner decision, gate 2.14). Expected registry change: none, the field stays `code-defined`.
- Result: none, read-only

### alarm_enable

- Offset 535, u16. Value in the fw1022 dump: 15.
- Test value: the current value with one defined bit toggled.
- Safe loop state: base. No Warning or Alarm is active: `alarm_bits` and `alarm_bits_latched` read 0.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### alarm_buzzer

- Offset 537, u16. Value in the fw1022 dump: 1.
- Test value: the current value with one defined bit toggled.
- Safe loop state: base. No Warning or Alarm is active: `alarm_bits` and `alarm_bits_latched` read 0.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### fill_level_warning_threshold

- Offset 539, i16. Value in the fw1022 dump: 6042.
- Test value: the current value plus or minus one raw step (0.01 %).
- Safe loop state: base. No Warning or Alarm is active: `alarm_bits` and `alarm_bits_latched` read 0.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### fill_level_alarm_threshold

- Offset 545, i16. Value in the fw1022 dump: 2000.
- Test value: the current value plus or minus one raw step (0.01 %).
- Safe loop state: base. No Warning or Alarm is active: `alarm_bits` and `alarm_bits_latched` read 0.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### leak_rate_warning_threshold

- Offset 551, i16. Value in the fw1022 dump: 150.
- Test value: the current value plus or minus one raw step (1 ml/h).
- Safe loop state: base. No Warning or Alarm is active: `alarm_bits` and `alarm_bits_latched` read 0.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### fill_level_warning_response_action

- Offset 541, u8. Value in the fw1022 dump: 0.
- Test value: 0 (none) when the current value is not 0, else 3 (USB key sequence). Never 1 or 2 (power switch).
- Safe loop state: base. No Warning or Alarm is active: `alarm_bits` and `alarm_bits_latched` read 0.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### fill_level_warning_response_modifiers

- Offset 542, u8. Value in the fw1022 dump: 0.
- Test value: the current value with one defined bit toggled.
- Safe loop state: base. No Warning or Alarm is active: `alarm_bits` and `alarm_bits_latched` read 0.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### fill_level_warning_response_key

- Offset 543, u16. Value in the fw1022 dump: 0.
- Test value: the next allowed code after the current value.
- Safe loop state: base. No Warning or Alarm is active: `alarm_bits` and `alarm_bits_latched` read 0.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### fill_level_alarm_response_action

- Offset 547, u8. Value in the fw1022 dump: 0.
- Test value: 0 (none) when the current value is not 0, else 3 (USB key sequence). Never 1 or 2 (power switch).
- Safe loop state: base. No Warning or Alarm is active: `alarm_bits` and `alarm_bits_latched` read 0.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### fill_level_alarm_response_modifiers

- Offset 548, u8. Value in the fw1022 dump: 8.
- Test value: the current value with one defined bit toggled.
- Safe loop state: base. No Warning or Alarm is active: `alarm_bits` and `alarm_bits_latched` read 0.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### fill_level_alarm_response_key

- Offset 549, u16. Value in the fw1022 dump: 15.
- Test value: the next allowed code after the current value.
- Safe loop state: base. No Warning or Alarm is active: `alarm_bits` and `alarm_bits_latched` read 0.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### leak_rate_warning_response_action

- Offset 553, u8. Value in the fw1022 dump: 0.
- Test value: 0 (none) when the current value is not 0, else 3 (USB key sequence). Never 1 or 2 (power switch).
- Safe loop state: base. No Warning or Alarm is active: `alarm_bits` and `alarm_bits_latched` read 0.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### leak_rate_warning_response_modifiers

- Offset 554, u8. Value in the fw1022 dump: 8.
- Test value: the current value with one defined bit toggled.
- Safe loop state: base. No Warning or Alarm is active: `alarm_bits` and `alarm_bits_latched` read 0.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### leak_rate_warning_response_key

- Offset 555, u16. Value in the fw1022 dump: 15.
- Test value: the next allowed code after the current value.
- Safe loop state: base. No Warning or Alarm is active: `alarm_bits` and `alarm_bits_latched` read 0.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### leak_rate_alarm_1_response_action

- Offset 557, u8. Value in the fw1022 dump: 0.
- Test value: 0 (none) when the current value is not 0, else 3 (USB key sequence). Never 1 or 2 (power switch).
- Safe loop state: base. No Warning or Alarm is active: `alarm_bits` and `alarm_bits_latched` read 0.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### leak_rate_alarm_1_response_modifiers

- Offset 558, u8. Value in the fw1022 dump: 8.
- Test value: the current value with one defined bit toggled.
- Safe loop state: base. No Warning or Alarm is active: `alarm_bits` and `alarm_bits_latched` read 0.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### leak_rate_alarm_1_response_key

- Offset 559, u16. Value in the fw1022 dump: 7.
- Test value: the next allowed code after the current value.
- Safe loop state: base. No Warning or Alarm is active: `alarm_bits` and `alarm_bits_latched` read 0.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### leak_rate_alarm_2_response_action

- Offset 561, u8. Value in the fw1022 dump: 2.
- Test value: 0 (none) when the current value is not 0, else 3 (USB key sequence). Never 1 or 2 (power switch).
- Safe loop state: base. No Warning or Alarm is active: `alarm_bits` and `alarm_bits_latched` read 0.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### leak_rate_alarm_2_response_modifiers

- Offset 562, u8. Value in the fw1022 dump: 8.
- Test value: the current value with one defined bit toggled.
- Safe loop state: base. No Warning or Alarm is active: `alarm_bits` and `alarm_bits_latched` read 0.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### leak_rate_alarm_2_response_key

- Offset 563, u16. Value in the fw1022 dump: 7.
- Test value: the next allowed code after the current value.
- Safe loop state: base. No Warning or Alarm is active: `alarm_bits` and `alarm_bits_latched` read 0.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### system_alarm_response_action

- Offset 565, u8. Value in the fw1022 dump: 0.
- Test value: 0 (none) when the current value is not 0, else 3 (USB key sequence). Never 1 or 2 (power switch).
- Safe loop state: base. No Warning or Alarm is active: `alarm_bits` and `alarm_bits_latched` read 0.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### system_alarm_response_modifiers

- Offset 566, u8. Value in the fw1022 dump: 8.
- Test value: the current value with one defined bit toggled.
- Safe loop state: base. No Warning or Alarm is active: `alarm_bits` and `alarm_bits_latched` read 0.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### system_alarm_response_key

- Offset 567, u16. Value in the fw1022 dump: 7.
- Test value: the next allowed code after the current value.
- Safe loop state: base. No Warning or Alarm is active: `alarm_bits` and `alarm_bits_latched` read 0.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### loop_height

- Offset 569, u8. Value in the fw1022 dump: 25.
- Test value: the current value plus or minus one raw step (1 cm).
- Safe loop state: base. Pressure stable within 2 mbar of the setpoint for 5 min.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### refill_duration_warning

- Offset 570, u8. Value in the fw1022 dump: 5.
- Test value: the current value plus or minus one raw step (1 s).
- Safe loop state: base. No Warning or Alarm is active: `alarm_bits` and `alarm_bits_latched` read 0.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### shield_flags

- Offset 714, i16. Value in the fw1022 dump: 127.
- Test value: the current value with one defined bit toggled.
- Safe loop state: base. No time-controlled vacuum window starts within 15 min of the check, also with the test value.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### vacuum_timer_start_hour

- Offset 716, u8, copy 0 of 2. Value in the fw1022 dump: 9.
- Test value: the current value plus or minus one raw step (1 h).
- Safe loop state: base. No time-controlled vacuum window starts within 15 min of the check, also with the test value.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### vacuum_timer_start_minute

- Offset 717, u8, copy 0 of 2. Value in the fw1022 dump: 0.
- Test value: the current value plus or minus one raw step (1 min).
- Safe loop state: base. No time-controlled vacuum window starts within 15 min of the check, also with the test value.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### vacuum_timer_end_hour

- Offset 720, u8, copy 0 of 2. Value in the fw1022 dump: 9.
- Test value: the current value plus or minus one raw step (1 h).
- Safe loop state: base. No time-controlled vacuum window starts within 15 min of the check, also with the test value.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### vacuum_timer_end_minute

- Offset 721, u8, copy 0 of 2. Value in the fw1022 dump: 1.
- Test value: the current value plus or minus one raw step (1 min).
- Safe loop state: base. No time-controlled vacuum window starts within 15 min of the check, also with the test value.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### default_mode

- Offset 724, u8. Value in the fw1022 dump: 3.
- Test value: 2 (MONITOR) when the current value is not 2, else 1 (SHIELD).
- Safe loop state: base. The replug step powers the device up in the Mode of the test value. Set the Mode back after the restore.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### test_pressure

- Offset 725, i16. Value in the fw1022 dump: 3000.
- Test value: the current value plus or minus one raw step (0.1 mbar).
- Safe loop state: base. No Action runs, and none starts during the check.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### deaeration_pressure

- Offset 727, i16. Value in the fw1022 dump: 3000.
- Test value: the current value plus or minus one raw step (0.1 mbar).
- Safe loop state: base. No Action runs, and none starts during the check.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### refill_pressure

- Offset 729, i16. Value in the fw1022 dump: 3000.
- Test value: the current value plus or minus one raw step (0.1 mbar).
- Safe loop state: base. No Action runs, and none starts during the check.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### shield_pressure_low

- Offset 731, i16. Value in the fw1022 dump: 3750.
- Test value: the current value plus or minus one raw step (0.1 mbar).
- Safe loop state: base. Pressure stable within 2 mbar of the setpoint for 5 min. The written value keeps lower limit < setpoint < upper limit.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### shield_pressure_low_standby

- Offset 733, i16. Value in the fw1022 dump: 800.
- Test value: the current value plus or minus one raw step (0.1 mbar).
- Safe loop state: base. Pressure stable within 2 mbar of the setpoint for 5 min. The written value keeps lower limit < setpoint < upper limit.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### shield_pressure_working_range

- Offset 735, i16. Value in the fw1022 dump: 500.
- Test value: the current value plus or minus one raw step (0.1 mbar).
- Safe loop state: base. Pressure stable within 2 mbar of the setpoint for 5 min. The written value keeps lower limit < setpoint < upper limit.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### shield_pressure_max_range

- Offset 737, i16. Value in the fw1022 dump: 400.
- Test value: the current value plus or minus one raw step (0.1 mbar).
- Safe loop state: base. Pressure stable within 2 mbar of the setpoint for 5 min. The written value keeps lower limit < setpoint < upper limit.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### monitor_pressure_low

- Offset 739, i16. Value in the fw1022 dump: 500.
- Test value: the current value plus or minus one raw step (0.1 mbar).
- Safe loop state: base. Pressure stable within 2 mbar of the setpoint for 5 min. The written value keeps lower limit < setpoint < upper limit.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### monitor_pressure_low_standby

- Offset 741, i16. Value in the fw1022 dump: 500.
- Test value: the current value plus or minus one raw step (0.1 mbar).
- Safe loop state: base. Pressure stable within 2 mbar of the setpoint for 5 min. The written value keeps lower limit < setpoint < upper limit.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### monitor_pressure_working_range

- Offset 743, i16. Value in the fw1022 dump: 300.
- Test value: the current value plus or minus one raw step (0.1 mbar).
- Safe loop state: base. Pressure stable within 2 mbar of the setpoint for 5 min. The written value keeps lower limit < setpoint < upper limit.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open

### monitor_pressure_max_range

- Offset 745, i16. Value in the fw1022 dump: 200.
- Test value: the current value plus or minus one raw step (0.1 mbar).
- Safe loop state: base. Pressure stable within 2 mbar of the setpoint for 5 min. The written value keeps lower limit < setpoint < upper limit.
- Expected registry change: `code-defined` to `verified-write` on pass, `failed` on fail.
- Result: open
