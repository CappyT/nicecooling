# LEAKSHIELD protocol

This document describes the USB protocol of the Aqua Computer LEAKSHIELD (USB `0c70:f014`), firmware 1022, structure 100. The source is the code of aquasuite X.96 and X.75 (ADR-0001), checked against the dumps in `tests/fixtures/leakshield/`. All names are our own. [docs/re/leakshield-code-paths.md](../re/leakshield-code-paths.md) gives the aquasuite code for each item as `P1` to `P10`.

The field registry [`crates/nicecooling-protocol/registry/leakshield.toml`](../../crates/nicecooling-protocol/registry/leakshield.toml) holds the risk class and the verification state of each report 3 field, the volatile mask and the report 2 commands. This document does not repeat them.

## Conventions

### Offsets

An offset counts from the report ID byte, which is at offset 0. The fixtures hold the report ID byte, so a fixture offset and a protocol offset are the same number.

### Byte order

Every multi-byte field is big-endian. This applies to all reports, the Feed message and the bulk device-info reply.

### Report 3 CRC

The report 3 CRC is CRC-16/USB over offsets 1 to 747 (the 747 struct bytes, without the report ID). It is stored big-endian at offsets 748-749. CRC-16/USB: polynomial 0x8005, reflected input and output, initial value 0xFFFF, final XOR 0xFFFF. The fw1019 and fw1022 dumps both have the CRC 0xECF7.

Report 8 and the Feed message use the same algorithm (sections Report 8 and Feed message). Report 2 uses it over its 8 payload bytes.

### Serial number

The serial number is report 1 bytes 3-6, as two big-endian u16. The text form is `<first>-<second>`. The reference device has `20417-31072`.

### Firmware version

The firmware version is report 1 bytes 13-14, big-endian u16. The registry is keyed on firmware 1022. The structure id is report 1 bytes 1-2 and report 3 bytes 1-2. It is 100 on firmware 1019 and 1022 (P1).

## Transport

| Interface | Item | Direction | Size | Content |
|---|---|---|---|---|
| 1 (HID) | report 1 | input, sent by the device | 837 | Device info, system state, sensors, event list |
| 1 (HID) | report 2 | output | 11 | Commands |
| 1 (HID) | report 3 | feature read and write | 750 | Settings |
| 1 (HID) | report 5 | feature read | 1028 | Display screenshot |
| 1 (HID) | report 8 | feature read and write | 3091 | Names, start image, icons |
| 0 (vendor, bulk OUT 0x02, IN 0x81) | Feed message | host to device | 51 | Software sensor values |
| 0 (vendor, bulk) | device-info request | host to device, then device to host | 2, reply 12 | Current device profile |

Interface 2 is a second HID interface with a 16-byte interrupt endpoint. This document does not cover it.

## Report 1

The device sends report 1 by itself. Offsets are absolute.

### Device info and system state

| Offset | Type | Name | Scale or meaning |
|---|---|---|---|
| 1 | u16 | `structure_number` | 100 |
| 3 | u16 x 2 | `serial` | Serial number (Conventions) |
| 7 | u16 | `hardware_revision` | |
| 9 | u16 | `device_kind` | |
| 11 | u16 | `bootloader_version` | |
| 13 | u16 | `firmware_version` | Firmware version (Conventions) |
| 15 | u32 | `status_bits` | Bit 2: device clock valid |
| 19 | u8 | `active_profile` | Current device profile |
| 20 | u32 | `uptime` | Not decoded. The fw1022 dump reads 1278 |
| 24 | u32 | `power_up_count` | |
| 28 | u32 | `total_runtime` | |
| 32 | u16 | `last_button_press` | |
| 34 | u8 | `display_language` | 0 English, 1 German |
| 35 | u32 | `device_clock` | Seconds since 2009-01-01 00:00 UTC |

### Data

| Offset | Type | Name | Scale or meaning |
|---|---|---|---|
| 39 | u8 x 5, u16, u16, u8 | `local_time` | Second, minute, hour, day, month, year, day of year, weekday. In the fw1022 dump it equals `device_clock` plus `clock_timezone` |
| 49 | u8 | `settings_dirty` | |
| 50 | u8 | `profile_number` | |
| 51 | u16 x 4 | `raw_adc_values` | |
| 59 | 20 x 10 bytes | `sensor[0..19]` | Section Device sensors |
| 259 | i16 | `supply_voltage` | 0.01 V |
| 261 | i16 | `pump_speed_reading` | rpm |
| 263 | i16 | `pump_current_draw` | 0.001 A |
| 265 | i16 | `daily_pump_duration` | |
| 267 | i16 | `pump_speed_setpoint` | rpm |
| 269 | i16 | `fill_valve_setpoint` | 0.01 |
| 271 | i16 | `release_valve_setpoint` | 0.01 |
| 273 | i32 | `pressure_raw_32` | 0.01 mbar |
| 277 | i32 | `pressure_raw_32_filtered` | 0.01 mbar |
| 281 | i16 | `pressure_raw` | 0.1 mbar |
| 283 | i16 | `pressure_offset` | 0.1 mbar |
| 285 | i16 | `pressure` | 0.1 mbar |
| 287 | i16 | `membrane_state` | 0 invalid, 1 good, 2 check |
| 289 | i16 | `pump_head_pressure` | 0.1 mbar |
| 291 | i16 | `pressure_low` | 0.1 mbar |
| 293 | i16 | `pressure_setpoint` | 0.1 mbar |
| 295 | i16 | `pressure_high` | 0.1 mbar |
| 297 | i16 | `mode` | Mode and Action codes (section Report 2) |
| 299 | i16 | `procedure_state` | 0 init, 1 run, 2 release, 3 pump, 4 stabilize, 5 finished, 6 pump cool-down, 7 measure, 8 measure error, 9 protected, 10 release run, 11 measure prepare, 12 unprotected |
| 301 | i16 | `pressure_leak_rate_average` | 0.1 mbar/min |
| 303 | i16 | `volume_leak_rate_average` | ml/h |
| 305 | i16 | `pressure_leak_rate` | 0.1 mbar/min |
| 307 | i16 | `volume_leak_rate` | ml/h |
| 309 | i16 | `air_volume` | ml |
| 311 | i16 | `water_volume` | ml |
| 313 | i16 | `reservoir_volume` | ml |
| 315 | i16 | `fill_level` | 0.01 % |
| 317 | i16 | `progress_1` | 0.01 % |
| 319 | i16 | `progress_2` | 0.01 % |
| 321 | u32 | `pump_runtime` | |
| 325 | u32 | `pump_short_cycles` | |
| 329 | u32 | `pump_revolution_count` | |
| 333 | u32 | `fill_valve_cycles` | |
| 337 | u32 | `release_valve_cycles` | |
| 341 | i16 x 6 | `leak_rate_statistics` | |
| 353 | 7 x 16 bytes | `refill_history` | Each: time u32 (`device_clock` format), duration u32, pump revolutions u32, volume i32 (scale 0.1) |
| 465 | 15 x 20 bytes | `event_list` | Section Event list |
| 765 | u32 | `alarm_bits` | Section Warnings and Alarms |
| 769 | u32 | `alarm_bits_latched` | Same bits, kept until a reset |
| 773 | i16 x 32 | `debug` | |

A value of 32767 (0x7FFF) in an i16 field means "no value".

### Device sensors

Each of the 20 sensors has 10 bytes at offset 59 + 10 x n: unit code (u8), name index into report 8 (u8), value, minimum, maximum and average (4 x i16). The unit code sets the scale (section Unit codes). 32767 means no value. The names in report 8 of the reference device give the slots:

| Sensor | Content | Source |
|---|---|---|
| 0 | Pressure | device |
| 1 | Fill level | device |
| 2 | Volume change | device |
| 3 | Pressure change | device |
| 4 | Pump speed | Feed slot 0 |
| 5 | Flow | Feed slot 1 |
| 6-7 | Spare | Feed slots 2-3 |
| 8-19 | Software sensors 1-12 | Feed slots 4-15 |

The kernel driver `aquacomputer_d5next` shows sensor 4 as "User-Provided Pump Speed" (offset 101) and sensor 5 as "User-Provided Flow" (offset 111) (P3). The map from Feed slot to sensor comes from the names and the units of the dump, not from the code (open question 3).

### Event list

The event list has 15 entries of 20 bytes at offset 465: time u32 (`device_clock` format), entry id u8, event code u8, Mode i16, procedure state i16, pressure i16 (0.1 mbar), pressure leak rate i16, volume leak rate i16, value 1 i16, value 2 i16. Event code 255 marks an empty entry. The event codes (P3):

| Code | Event | Code | Event |
|---|---|---|---|
| 0 | system start | 15 | test fill error |
| 1 | Mode SHIELD | 16 | test leak |
| 2 | Mode MONITOR | 17 | test finished |
| 3 | Mode RELEASE | 18 | refill finished |
| 4 | Action measure fill level | 19 | shield setpoint changed |
| 5 | Action deaerate | 20 | shield pressure low |
| 6 | Action test | 21 | shield pressure high |
| 7 | Action refill | 22 | shield refill |
| 8 | release | 23 | shield pump cool-down |
| 9 | sensor calibration | 24 | shield pump cool-down stopped |
| 10 | release run | 25 | shield pump finished |
| 11 | fill level measurement start | 26 | shield pump maximum runtime |
| 12 | fill level measurement error | 27 | test leak prepare |
| 13 | fill level measurement finished | 100-110 | Warnings and Alarms, same order as the bits below |
| 14 | deaeration finished | | |

### Warnings and Alarms

`alarm_bits` bits and the event identifiers of `docs/api.md`:

| Bit | Event identifier | Severity |
|---|---|---|
| 0 | `leakshield.fill-level-low-warning` | warning |
| 1 | `leakshield.fill-level-low-alarm` | alarm |
| 2 | `leakshield.leak-warning` | warning |
| 3 | `leakshield.pump-idle-time-warning` | warning |
| 4 | `leakshield.pump-idle-time-alarm` | alarm |
| 5 | `leakshield.pump-runtime-alarm` | alarm |
| 6 | `leakshield.high-pressure-drop` | alarm |
| 7 | `leakshield.supply-voltage` | alarm |
| 8 | `leakshield.pump-stall` | alarm |
| 9 | `leakshield.pump-overcurrent` | alarm |
| 10 | `leakshield.pump-low-volume` | alarm |

The severity follows the event name: the code names bits 0, 2 and 3 "warning" and the others "alarm".

## Report 2

Report 2 is an output report of 11 bytes: report ID 2, command u32, parameter u32, CRC-16/USB over the 8 bytes of command and parameter. The registry lists the commands and their parameters (P4). The save-settings command is `02 00000002 00000000 34C6`.

Mode and Action use one transport: command 12 (`set_mode`) with the code as the parameter. aquasuite puts a 25 ms delay before it.

| Code | Mode or Action |
|---|---|
| 1 | Mode SHIELD |
| 2 | Mode MONITOR |
| 3 | Mode RELEASE |
| 4 | Action measure fill level |
| 5 | Action deaerate |
| 6 | Action test |
| 7 | Action refill |
| 100 | Factory test. Never send it. |

## Report 3

Report 3 holds the settings: 750 bytes, the report ID, the 747-byte struct and the CRC. Bytes 1-2 are the structure id. The table gives each field with its offset, type, repeat, scale, range, the default that aquasuite code sets, and the value in the fw1022 dump. A repeat `n x s` means n copies, s bytes apart. A second repeat is a nested copy inside each first copy. The range is in engineering units (raw value times scale). "Type only" means the code defines no range.

aquasuite reads the struct, changes the fields of the user, and writes back every other byte as read (P2). Its GUI keeps the shield and monitor setpoint at least 20 mbar above the lower limit and the upper limit at least 30 mbar above the setpoint. The fw1022 dump has a monitor upper range of 20.0 mbar, so the registry range of that field is the full span of the limits.

| Field id | Offset | Type | Repeat | Scale | Range | aquasuite default | Fixture fw1022 | Meaning |
|---|---|---|---|---|---|---|---|---|
| `signal_output_config` | 3 | u8 |  |  | type only |  | 0 | Setting of the signal output. The code does not define its values; aquasuite keeps the byte as read. |
| `fill_level_calibration` | 4 | i16 |  | 0.01 | -20 to 20 % |  | 0 | Correction of the fill level. |
| `reservoir_model` | 6 | u8 |  |  | codes 0-11, 255 |  | 255 | Reservoir preset: 0 undefined, 1-11 Aqua Computer reservoirs, 255 custom. |
| `reservoir_air_volume_full` | 7 | i16 |  |  | 0 to 32767 ml |  | 65 | Air volume of the reservoir at fill level 100 %. |
| `reservoir_air_volume_empty` | 9 | i16 |  |  | 0 to 32767 ml |  | 260 | Air volume of the reservoir at fill level 0 %. |
| `reservoir_total_volume` | 11 | i16 |  |  | 0 to 32767 ml |  | 270 | Total volume of the reservoir. |
| `pump_model` | 13 | u8 |  |  | codes 0-8, 255 |  | 1 | Pump preset: 0 undefined, 1 D5, 2 aquastream, 3-5 DDC, 6-8 Eheim, 255 custom. |
| `pump_flags` | 14 | u8 |  |  | mask 0x3 |  | 0 | Bit 0: custom pump. Bit 1: dual pump. |
| `pump_max_speed` | 15 | i16 |  |  | 0 to 32767 rpm | 4800 | 4800 | Pump speed at full duty ("max speed"). |
| `pump_max_pressure` | 17 | i16 |  | 0.1 | 0 to 3276.7 mbar | 365.0 | 3750 | Pump pressure at full speed ("max pressure"). |
| `pump_start_speed` | 19 | i16 |  |  | 0 to 32767 rpm | 4800 | 2500 | Pump speed that the device assumes without a Feed value ("start speed"). aquasuite keeps it at or below `pump_max_speed`. |
| `pump_start_flow` | 21 | i16 |  | 0.1 | 0 to 3276.7 l/h | 100.0 | 1300 | Flow that the device assumes without a Feed value ("start flow"). |
| `sensor_log_interval` | 23 | u8 | 20 x 2 |  | codes 0-7 |  | [1, 1, 1, 1, 1, 1, ...] | Log interval of each device sensor: 0 0.5 s, 1 1 s, 2 5 s, 3 10 s, 4 30 s, 5 1 min, 6 5 min, 7 10 min. |
| `sensor_unit` | 24 | u8 | 20 x 2 |  | codes 0-47 |  | [0, 0, 0, 0, 3, 12, ...] | Unit code of each device sensor (unit code table). |
| `display_page_time` | 63 | u16 |  | 0.1 | 0 to 6553.5 s |  | 100 | Time that each display page shows. |
| `display_brightness_active` | 65 | u8 |  |  | codes 0-3 |  | 2 | 0 maximum, 1 mid, 2 low, 3 off. |
| `display_brightness_idle` | 66 | u8 |  |  | codes 0-3 |  | 2 | 0 maximum, 1 mid, 2 low, 3 off. |
| `display_idle_delay` | 67 | u16 |  |  | type only | 2400 | 3100 | Delay before the display goes idle. aquasuite always writes 2400. Unit not defined in the code. |
| `clock_flags` | 69 | u8 |  |  | mask 0x7 |  | 1 | Bit 0: daylight saving time on. Bit 1: 12 h format. Bit 2: daylight saving region US (clear: EU). |
| `clock_timezone` | 70 | i16 |  | 0.1 | -12 to 14 h |  | 10 | Time zone offset of the device local time. |
| `display_flags` | 72 | u8 |  |  | mask 0xF |  | 0 | Bit 0: rotation off. Bit 1: display standby allowed. Bit 2: invert. Bit 3: automatic invert off. |
| `page_template` | 73 | u8 | 10 x 46 |  | codes 0-13, 255 |  | [13, 2, 1, 3, 3, 1, ...] | Page layout: 0 logo, 1 chart, 2-6 values, 7 time, 8-10 bars, 11 value and bar, 12 value and time, 13 Leakshield state, 255 none. |
| `page_options` | 74 | u8 | 10 x 46 |  | mask 0x3 |  | [3, 3, 3, 3, 3, 3, ...] | Bit 0: show the page in standby. Bit 1: hide the page in normal operation. |
| `page_icons` | 75 | u8[4] | 10 x 46 |  | type only |  | [255, 255, 255, 255, 2, 255, ...] | Index into the icons of report 8, 255 none. |
| `page_names` | 79 | u8[8] | 10 x 46 |  | type only |  | [255, 255, 255, 255, 255, 255, ...] | Index into the names of report 8, 255 none. |
| `page_sources` | 87 | u8[8] | 10 x 46 |  | type only |  | [255, 255, 255, 255, 255, 255, ...] | Value source of each page slot, 255 none. |
| `page_precisions` | 95 | u8[8] | 10 x 46 |  | type only |  | [0, 0, 0, 0, 0, 0, ...] | Decimal places of each page slot. |
| `page_values` | 103 | i16[8] | 10 x 46 |  | type only |  | [0, 100, 0, 100, 0, 100, ...] | Template parameters of each page slot. |
| `standby_conditions` | 533 | u8 |  |  | mask 0xB |  | 11 | Bit 0: standby without USB connection. Bit 1: standby on USB suspend. Bit 3: leave standby on alarm. |
| `alarm_start_delay` | 534 | u8 |  |  | type only |  | 5 | Alarm start timeout. aquasuite keeps the byte as read. Unit not defined in the code. |
| `alarm_enable` | 535 | u16 |  |  | mask 0xF |  | 15 | Bit 0: fill level warning. Bit 1: fill level alarm. Bits 2-3: leak rate warning. |
| `alarm_buzzer` | 537 | u16 |  |  | mask 0xF |  | 1 | Buzzer for the same bits as `alarm_enable`. |
| `fill_level_warning_threshold` | 539 | i16 |  | 0.01 | 0 to 100 % |  | 6042 | Fill level below which the warning starts. |
| `fill_level_alarm_threshold` | 545 | i16 |  | 0.01 | 0 to 100 % |  | 2000 | Fill level below which the alarm starts. |
| `leak_rate_warning_threshold` | 551 | i16 |  |  | 50 to 200 ml/h |  | 150 | Leak rate above which the warning starts. |
| `fill_level_warning_response_action` | 541 | u8 |  |  | codes 0-3 |  | 0 | Device response: 0 none, 1 power switch short, 2 power switch long, 3 USB key sequence. |
| `fill_level_warning_response_modifiers` | 542 | u8 |  |  | mask 0xF |  | 0 | Key modifiers of the USB key sequence: bit 0 ctrl, bit 1 shift, bit 2 alt, bit 3 windows. |
| `fill_level_warning_response_key` | 543 | u16 |  |  | codes 0-2004, 110 codes (list in the registry) |  | 0 | Key code of the USB key sequence. Codes 4-115 follow the USB HID keyboard usages. |
| `fill_level_alarm_response_action` | 547 | u8 |  |  | codes 0-3 |  | 0 | Device response: 0 none, 1 power switch short, 2 power switch long, 3 USB key sequence. |
| `fill_level_alarm_response_modifiers` | 548 | u8 |  |  | mask 0xF |  | 8 | Key modifiers of the USB key sequence: bit 0 ctrl, bit 1 shift, bit 2 alt, bit 3 windows. |
| `fill_level_alarm_response_key` | 549 | u16 |  |  | codes 0-2004, 110 codes (list in the registry) |  | 15 | Key code of the USB key sequence. Codes 4-115 follow the USB HID keyboard usages. |
| `leak_rate_warning_response_action` | 553 | u8 |  |  | codes 0-3 |  | 0 | Device response: 0 none, 1 power switch short, 2 power switch long, 3 USB key sequence. |
| `leak_rate_warning_response_modifiers` | 554 | u8 |  |  | mask 0xF |  | 8 | Key modifiers of the USB key sequence: bit 0 ctrl, bit 1 shift, bit 2 alt, bit 3 windows. |
| `leak_rate_warning_response_key` | 555 | u16 |  |  | codes 0-2004, 110 codes (list in the registry) |  | 15 | Key code of the USB key sequence. Codes 4-115 follow the USB HID keyboard usages. |
| `leak_rate_alarm_1_response_action` | 557 | u8 |  |  | codes 0-3 |  | 0 | Device response: 0 none, 1 power switch short, 2 power switch long, 3 USB key sequence. |
| `leak_rate_alarm_1_response_modifiers` | 558 | u8 |  |  | mask 0xF |  | 8 | Key modifiers of the USB key sequence: bit 0 ctrl, bit 1 shift, bit 2 alt, bit 3 windows. |
| `leak_rate_alarm_1_response_key` | 559 | u16 |  |  | codes 0-2004, 110 codes (list in the registry) |  | 7 | Key code of the USB key sequence. Codes 4-115 follow the USB HID keyboard usages. |
| `leak_rate_alarm_2_response_action` | 561 | u8 |  |  | codes 0-3 |  | 2 | Device response: 0 none, 1 power switch short, 2 power switch long, 3 USB key sequence. |
| `leak_rate_alarm_2_response_modifiers` | 562 | u8 |  |  | mask 0xF |  | 8 | Key modifiers of the USB key sequence: bit 0 ctrl, bit 1 shift, bit 2 alt, bit 3 windows. |
| `leak_rate_alarm_2_response_key` | 563 | u16 |  |  | codes 0-2004, 110 codes (list in the registry) |  | 7 | Key code of the USB key sequence. Codes 4-115 follow the USB HID keyboard usages. |
| `system_alarm_response_action` | 565 | u8 |  |  | codes 0-3 |  | 0 | Device response: 0 none, 1 power switch short, 2 power switch long, 3 USB key sequence. |
| `system_alarm_response_modifiers` | 566 | u8 |  |  | mask 0xF |  | 8 | Key modifiers of the USB key sequence: bit 0 ctrl, bit 1 shift, bit 2 alt, bit 3 windows. |
| `system_alarm_response_key` | 567 | u16 |  |  | codes 0-2004, 110 codes (list in the registry) |  | 7 | Key code of the USB key sequence. Codes 4-115 follow the USB HID keyboard usages. |
| `loop_height` | 569 | u8 |  |  | 0 to 200 cm |  | 25 | Height of the loop. |
| `refill_duration_warning` | 570 | u8 |  |  | 5 to 240 s | 5 | 5 | Pump duration of a refill above which a warning starts. |
| `led_strip_brightness` | 571 | u8 |  | 1/2.55 | 0 to 100 % |  | 255 | Brightness of the LED strip. |
| `led_strip_flags` | 572 | u16 |  |  | mask 0x6 |  | 4 | Bit 1: strip off. Bit 2: strip on in standby. |
| `led_controller_strip` | 574 | u8 | 2 x 70 |  | type only |  | [0, 0] | Strip of the LED controller. |
| `led_controller_first_led` | 575 | u8 | 2 x 70 |  | type only |  | [0, 0] | First LED of the LED controller. |
| `led_controller_led_count` | 576 | u8 | 2 x 70 |  | type only |  | [6, 6] | Number of LEDs of the LED controller. |
| `led_controller_mode` | 577 | u8 | 2 x 70 |  | codes 0-5, 7-14, 18-19, 33 |  | [3, 0] | Effect: 0 unused, 1 fixed color, 2 breathing, 3 rainbow, 19 rainbow shift, 12 ambient, 4-5 blink, 7 sequential, 8-9 visor, 10-11 marquee, 14 candle, 18 switch, 13 bar, 33 gradient. |
| `led_controller_flags` | 578 | u16 | 2 x 70 |  | type only |  | [0, 0] | Bit 14: binding 1 off. Bit 15: binding 2 off. Other bits are effect options. |
| `led_controller_source` | 580 | i16 | 2 x 70 |  | type only |  | [-1, -1] | Sensor that drives the effect. |
| `led_controller_source_filter` | 582 | u8[2] | 2 x 70 |  | type only |  | [10, 15, 15, 25] | Rise and fall filter of the source. |
| `led_controller_binding_1_x1` | 584 | i16 | 2 x 70 |  | type only |  | [0, 0] | Lower source value of the binding. |
| `led_controller_binding_1_x2` | 586 | i16 | 2 x 70 |  | type only |  | [100, 1000] | Upper source value of the binding. |
| `led_controller_binding_1_y1` | 588 | u8 | 2 x 70 |  | type only |  | [1, 1] | Output at the lower source value. |
| `led_controller_binding_1_y2` | 589 | u8 | 2 x 70 |  | type only |  | [100, 100] | Output at the upper source value. |
| `led_controller_binding_2_x1` | 590 | i16 | 2 x 70 |  | type only |  | [0, 0] | Lower source value of the binding. |
| `led_controller_binding_2_x2` | 592 | i16 | 2 x 70 |  | type only |  | [100, 1000] | Upper source value of the binding. |
| `led_controller_binding_2_y1` | 594 | u8 | 2 x 70 |  | type only |  | [0, 1] | Output at the lower source value. |
| `led_controller_binding_2_y2` | 595 | u8 | 2 x 70 |  | type only |  | [255, 100] | Output at the upper source value. |
| `led_controller_parameters` | 596 | i16[12] | 2 x 70 |  | type only |  | [50, 100, 0, 0, 0, 0, ...] | Effect parameters. |
| `led_controller_color_hue` | 620 | u16 | 2 x 70, 6 x 4 |  | 0 to 360 deg |  | [0, 0, 0, 0, 0, 0, ...] | Hue of each of the 6 colors. |
| `led_controller_color_saturation` | 622 | u8 | 2 x 70, 6 x 4 |  | type only |  | [255, 0, 0, 0, 0, 0, ...] | Saturation of each of the 6 colors. |
| `led_controller_color_value` | 623 | u8 | 2 x 70, 6 x 4 |  | type only |  | [255, 0, 0, 0, 0, 0, ...] | Value (brightness) of each of the 6 colors. |
| `shield_flags` | 714 | i16 |  |  | mask 0xFF |  | 127 | Bit 1: time-controlled vacuum on. Bit 2: compensate the pressure equalisation membrane. Bit 3: adaptive pressure. Bit 4: default settings (clear: custom settings). Bit 5: standby pressure. Bit 6: permanent alarm buzzer. Bit 7: alarm buzzer off. Bit 0 is not defined in the code; aquasuite keeps it. |
| `vacuum_timer_start_hour` | 716 | u8 | 2 x 2 |  | 0 to 23 h |  | [9, 18] | Start of each of the 2 time-controlled vacuum windows. |
| `vacuum_timer_start_minute` | 717 | u8 | 2 x 2 |  | 0 to 59 min |  | [0, 0] | Start minute of each window. |
| `vacuum_timer_end_hour` | 720 | u8 | 2 x 2 |  | 0 to 23 h |  | [9, 18] | End of each window. aquasuite writes the start plus 1 min. |
| `vacuum_timer_end_minute` | 721 | u8 | 2 x 2 |  | 0 to 59 min |  | [1, 1] | End minute of each window. |
| `default_mode` | 724 | u8 |  |  | codes 1-3 |  | 3 | Mode after power-up and after an Action: 1 SHIELD, 2 MONITOR, 3 RELEASE. |
| `test_pressure` | 725 | i16 |  | 0.1 | 50 to 450 mbar |  | 3000 | Pressure of the Action (test). |
| `deaeration_pressure` | 727 | i16 |  | 0.1 | 50 to 450 mbar |  | 3000 | Pressure of the Action (deaeration). |
| `refill_pressure` | 729 | i16 |  | 0.1 | 50 to 450 mbar |  | 3000 | Pressure of the Action (refill). |
| `shield_pressure_low` | 731 | i16 |  | 0.1 | 30 to 430 mbar | 100.0 | 3750 | Lower pressure limit in SHIELD. |
| `shield_pressure_low_standby` | 733 | i16 |  | 0.1 | 30 to 430 mbar | 100.0 | 800 | Lower pressure limit in SHIELD while the PC is in standby. aquasuite keeps it at or below the lower limit. |
| `shield_pressure_working_range` | 735 | i16 |  | 0.1 | 0 to 420 mbar | 50.0 | 500 | Setpoint minus lower limit. |
| `shield_pressure_max_range` | 737 | i16 |  | 0.1 | 0 to 450 mbar | 50.0 | 400 | Upper limit minus setpoint. |
| `monitor_pressure_low` | 739 | i16 |  | 0.1 | 30 to 430 mbar | 100.0 | 500 | Lower pressure limit in MONITOR. |
| `monitor_pressure_low_standby` | 741 | i16 |  | 0.1 | 30 to 430 mbar | 100.0 | 500 | Lower pressure limit in MONITOR while the PC is in standby. aquasuite keeps it at or below the lower limit. |
| `monitor_pressure_working_range` | 743 | i16 |  | 0.1 | 0 to 420 mbar | 50.0 | 300 | Setpoint minus lower limit. |
| `monitor_pressure_max_range` | 745 | i16 |  | 0.1 | 0 to 450 mbar | 50.0 | 200 | Upper limit minus setpoint. |
| `device_profile` | 747 | u8 |  |  | type only |  | 1 | Number of the device profile. |

### Unit codes

`sensor_unit`, the device sensors and the Feed message use one unit code table. The number after the name is the decimal places of the raw value: 0 none, 1 none (1), 2 none (2), 3 rpm, 4 % (2), 5 fps, 6 mA, 7 A (2), 8 mV, 9 V (2), 10 degC (2), 11 degF (2), 12 l/h (1), 13 l/min (1), 14 gal/h (1), 15 gal/min (1), 16 Hz, 17 kHz, 18 MHz, 19 GHz (2), 20 mW, 21 W, 22 kW (2), 23 ms, 24 s (1), 25 min (1), 26 h (1), 27 days (1), 28 bit, 29 kbit, 30 Mbit, 31 Gbit (1), 32 mbar (1), 33 bar (2), 34 ratio (1), 35 MB, 36 GB (1), 37 TB (1), 38 bool, 39 g (1), 40 kg (1), 41 t (1), 42 deg (1), 43 humidity (2), 44 l, 45 conductivity (1), 46 mbar/min (1), 47 ml/h.

## Report 5

Report 5 is a feature read of 1028 bytes (P6). Byte 1 is the id of the page on the display (255: none). Bytes 2-1025 are a 128 x 64 monochrome image: 8 rows of 128 column bytes, bit 0 at the top of each byte. aquasuite does not use bytes 1026-1027. The aquasuite library has no write of report 5.

## Report 8

Report 8 holds the display assets: 3091 bytes, the report ID, a 3088-byte struct and a CRC-16/USB over the struct, big-endian at 3089-3090 (P7). The fw1022 dump has the CRC 0x7E32.

| Offset | Size | Content |
|---|---|---|
| 1 | 40 x 24 | Names, zero-terminated text. 0-19: names of the device sensors. 20-34: page names. 35-38: LED controllers 1-4. 39: LED strip |
| 961 | 1024 | Start image, 128 x 64 monochrome, same format as report 5 |
| 1985 | 16 x 69 | Icons |

aquasuite writes report 8 as one feature write with a new CRC. The write starts no save-settings command.

## Feed message

The Feed message goes to bulk endpoint 0x02 of interface 0 (P8). It has 51 bytes:

| Offset | Size | Content |
|---|---|---|
| 0 | 1 | 0x04 |
| 1 | 16 x 2 | Values of slots 0-15, i16, scaled by the unit code of the slot. 32767: no value |
| 33 | 16 | Unit code of each slot |
| 49 | 2 | CRC-16/USB over bytes 0-48, big-endian |

aquasuite fills each slot from a sensor that the user links, with a scale and an offset. It sends the message once per cycle of its device service, when the user has set up the 16 slots. The fw1022 dump and the kernel driver map slot 0 to the pump speed (unit 3, rpm) and slot 1 to the flow (unit 12, l/h with 1 decimal place). The Leakshield computes the loop pressure from them.

The out-of-tree driver also sends the Feed message. It must not run next to the daemon, because the device then gets two Feed messages from two sources.

## Bulk device-info request

The host writes the 2 bytes `C8 64` (200, 100) to bulk endpoint 0x02 and reads 12 bytes: request id 100, serial u32, hardware u16, firmware u16, current device profile u8 at byte 9, 2 spare bytes. aquasuite uses it to read the profile number before a profile write.

## Device profiles in aquasuite

aquasuite applies a device profile in this way (P9):

1. A profile change comes to the service as a message. The service writes it at once when no change waits and the last profile write is more than 2 s old. Else the change waits. The service writes only the newest waiting change, 1.5 s after the last write, and drops the others.
2. The service sends the bulk device-info request and reads the current device profile number. It writes the stored report 3 of the profile only when the numbers differ. It does not compare the bytes.
3. The write is the report 3 write of aquasuite: a 150 ms delay, then one feature write of the whole stored report. The save-settings command follows 4 s after the last report 3 write.

Fields written: the whole stored report 3, as stored. aquasuite does not keep any device byte.

Mode handling: the service does not check the Mode or a running Action before a profile write. The aquasuite maintenance mode blocks the hotkeys, so it blocks the profile switches by hotkey.

Automatic switching: the service switches a profile on a hotkey. At service start it applies the stored profile of the device. The unpacked code has no rule on processes, time or sensor values. The GUI assembly is not unpacked, so the GUI can hold more triggers.

Differences from design D14 and the profiles spec:

1. Comparison: aquasuite compares the profile number from the bulk reply. nicecooling compares report 3 outside the volatile mask.
2. Volatile bytes: aquasuite writes the stored bytes as stored. nicecooling keeps the volatile bytes of the device. The volatile mask is empty now, so this has no effect yet.
3. Mode: aquasuite writes also while an Action runs or the Mode is RELEASE. nicecooling blocks the Leakshield part with the Maintenance pause.
4. Rate limits: aquasuite has a 2 s and 1.5 s spacing between profile writes and keeps only the newest request. nicecooling has the Rule delay (30 s) and the Profile dwell (5 min).
5. Triggers: aquasuite switches on hotkeys and at service start. nicecooling has process, time and threshold rules and no hotkeys.
6. Firmware: aquasuite checks the device type and the serial number. nicecooling also refuses a write when the Profile and the device have different firmware.
7. Read-back: the aquasuite profile path does not compare a read-back. nicecooling reads back and offers the restore (design D11).

## Device clock

The device clock is `device_clock` in report 1: seconds since 2009-01-01 00:00 UTC, valid when bit 2 of `status_bits` is set (P5). The fw1022 dump has a valid clock that matches the dump time. The fw1019 dump has bit 2 clear and a clock from 2021. `local_time` is the device local time. The event list and the refill history use the `device_clock` format.

The `set_clock` command (37000) sets the clock. aquasuite converts the PC time to UTC first. Its service sets the clock at device access and every 30 min.

## Behavior when the Feed stops

The aquasuite code does not define what the device does when the Feed stops (P10). aquasuite has no logic for a missing Feed. Its only related action: when the PC suspends, it sends `suppress_alarm` with the parameter 3600 (90000 / 25).

Observations on the reference device, outside the code:

- fw1022 dump, after the firmware update in the Windows VM and with no Feed from Linux: sensor 4 reads 2500 rpm and sensor 5 reads 1300 (130.0 l/h). These are `pump_start_speed` (2500) and `pump_start_flow` (1300) of report 3. The kernel hwmon device `leakshield` showed the same values.
- fw1019 dump, and the live kernel hwmon on 2026-11-23: sensors 4 and 5 have no value (32767), and `fan2_input` and `fan3_input` return ENODATA.

Thus the device uses the start values of report 3 for some time without a Feed, but not always. The code names these fields "start" speed and flow. The condition and the duration are not known. Task 5.11 measures them.

## Open questions

| Question | Answer | Code path |
|---|---|---|
| Contents of reports 5 and 8 | Report 5: page id and the 128 x 64 screen image, read only. Report 8: 40 names, the start image and 16 icons, with a CRC | P6, P7 |
| Transport of Mode and Action | Report 2 command 12 with the Mode or Action code | P4 |
| Software sensor slots in the Feed message | 16 slots. The code fills them from user links and does not fix a slot meaning. The dump and the kernel driver give slot 0 pump speed, slot 1 flow, slots 2-3 spare, slots 4-15 the 12 Software sensors | P8 |
| Device clock offsets | `device_clock` at offset 35, validity bit 2 of `status_bits` at offset 15, `local_time` at offset 39 | P5 |
| Write path of display assets | Report 8 feature write of the whole struct with a new CRC. No save-settings command follows | P7 |
| Commands that clear the pump runtime and cancel an Action | The code does not define them. The library has no cancel command and no command that names the pump runtime. Candidates for the pump runtime: `reset_statistics` (13) and `service_reset` (25000). Their callers are in the GUI, which is not unpacked | P4 |
