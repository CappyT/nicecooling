# nicecooling

nicecooling controls the cooling of one Linux desktop: the fans and the pump on the motherboard, and the Aqua Computer LEAKSHIELD on the water loop.

## Devices

**Loop**:
One water circuit: its Pump channels, its flow sensor, its water temperature sensor and, when present, its Leakshield.
_Avoid_: circuit, system

**Leakshield**:
The Aqua Computer LEAKSHIELD on the reservoir. It keeps the loop under negative pressure and detects leaks.
_Avoid_: LS, leak shield

**Board**:
A motherboard, with its Super I/O chip and its embedded controller. The reference Board is the ASUS ROG CROSSHAIR VIII DARK HERO.
_Avoid_: mainboard, mobo

**Board file**:
The list of Channel and sensor names for one Board model.

## Leakshield operation

**Mode**:
The persistent operating state of the Leakshield: SHIELD, MONITOR or RELEASE.
_Avoid_: state, status

**Action**:
A temporary Leakshield procedure: Test, Measure fill level, Deaerate or Refill. When it ends, the Leakshield goes back to its Mode. The term never names an alarm response.
_Avoid_: task, job

**Warning**:
An event that signals a possible problem before it becomes an Alarm. The Leakshield and nicecooling both raise Warnings.

**Alarm**:
An event that signals a leak, a low fill level, a stopped pump, an unreachable Leakshield or a device fault. The Leakshield and nicecooling both raise Alarms.

**Alarm response**:
What nicecooling does for a Warning or an Alarm: none, notify, or poweroff after a delay.
_Avoid_: alarm action

**Device response**:
What the Leakshield itself does for one of its Warnings or Alarms: sound, light, a short or long press on its signal output, or a key sequence through its keyboard interface.
_Avoid_: device action

**Feed**:
The pump speed and the flow that the host sends to the Leakshield, so that it can compute the pressure of the loop.
_Avoid_: user-provided values

**Feed message**:
The message on the bulk endpoint of the Leakshield that carries the Feed.

**Software sensor**:
One of the 12 Leakshield slots that the host fills with a value, for display pages or lighting.

**Verified field**:
A Leakshield setting that aquasuite's code defines and that the registry marks as verified for the device model and firmware version: by a plausible read on the fixtures for a low-risk field, by a Verification write for a Safety field.

**Safety field**:
A Leakshield setting that affects the leak protection: Mode, pressure, reservoir, pump, alarms and standby behavior.

**Verification write**:
A supervised test during development that writes a known value to one field of the reference Leakshield, reads back the device settings, checks that only the expected bytes changed, and restores the previous value. The app has no control for it.

**Maintenance pause**:
The state in which Profile rules do not write the Leakshield, because an Action runs or the Mode is RELEASE set outside a Profile.

## Fan control

**Channel**:
One fan or pump output of the Board, with its speed reading and its Duty.
_Avoid_: header, port, pwm

**Duty**:
The output level of a Channel, in percent.
_Avoid_: pwm value, speed

**Fixed Duty**:
A Duty that the user sets on a Channel that has no Curve.

**Curve**:
A mapping from one Input to the Duty of one Channel.

**Input**:
The sensor that a Curve reads: a measured sensor or a Derived sensor.

**Derived sensor**:
A sensor that nicecooling computes from other sensors: a maximum, a mean or a difference such as water-to-air delta T.

**Calibration table**:
The list of points that converts the raw pulse rate of a flow sensor to a flow in L/h.

**Hardware curve**:
The 5-point curve stored in the Super I/O chip, which the chip applies without the host.
_Avoid_: Smart Fan, BIOS curve

**BIOS snapshot**:
The fan settings of all Channels, Hardware curves included, as the Board had them before nicecooling took control.

**Pump channel**:
The Channel that drives the loop pump.

**Pump floor**:
The minimum Duty of the Pump channel. No Curve goes below it.

## Profiles

**Profile**:
A named set of Curves and Leakshield settings that becomes active as one unit.
_Avoid_: preset, scene

**Default Profile**:
The Profile that always exists and cannot be deleted. It is active when no Profile rule wins and no manual activation exists.

**Profile rule**:
A condition that activates a Profile automatically.

**Rule delay**:
The time a Profile rule condition must stay true before the rule activates its Profile.

**Profile dwell**:
The minimum time a Profile that a Profile rule activated stays active.

**Immediate switching**:
The user option that removes the Rule delay and the Profile dwell, so that every rule change applies at once.
