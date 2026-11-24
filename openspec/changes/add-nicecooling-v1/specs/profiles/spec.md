# Profiles

## Purpose

Defines Profiles, the Leakshield slots, the Default Profile, the Rule groups that activate Profiles, the timing rules that stop fast switching, and the rules for the Leakshield write that a Profile switch causes.

## ADDED Requirements

### Requirement: Profile content
A Profile SHALL hold a set of Curves and a Leakshield slot number. The Leakshield settings SHALL NOT belong to the Profile: the daemon SHALL store them per Leakshield and per slot, as the whole settings report as the device produced it, with the binary assets such as display logos. The number of Profiles SHALL NOT be limited.

#### Scenario: Save a Profile
- **WHEN** the user saves a Profile with a display logo
- **THEN** the logo and the settings are in the slot of the Profile, and activating the Profile later writes them

#### Scenario: More than four Profiles
- **WHEN** the user creates a fifth Profile
- **THEN** the daemon creates it and it references one of the four slots

### Requirement: Leakshield slot
Each Profile SHALL reference one Leakshield slot, a number from 1 to 4. The number is the device profile number that the stored report 3 of the slot carries in `device_profile`. The daemon SHALL set the number in each report 3 that it stores in a slot. The Default Profile SHALL have slot 1, and its slot SHALL NOT change.

#### Scenario: Default Profile slot
- **WHEN** the daemon creates the Default Profile
- **THEN** its slot is 1

#### Scenario: New Profile
- **WHEN** the user creates a Profile as a copy of a Profile with slot 3
- **THEN** the new Profile has slot 3 and shares its Leakshield settings

#### Scenario: Stored report carries the number
- **WHEN** the daemon stores a report 3 in slot 2
- **THEN** the `device_profile` byte of the report is 2 and the CRC is valid

#### Scenario: Delete a Profile
- **WHEN** the user deletes a Profile with slot 3
- **THEN** the Leakshield settings of slot 3 stay

### Requirement: Initial Profiles and shared slots
At first start the daemon SHALL create the Default Profile and three more Profiles with the slots 2, 3 and 4. Several Profiles MAY have the same slot, and they then share the Leakshield settings of the slot. A new Profile SHALL take the slot of the Profile it copies, and the slot of the active Profile when it is created from the current state.

#### Scenario: First start
- **WHEN** the daemon starts with no state
- **THEN** it creates the Default Profile with slot 1 and three Profiles with the slots 2, 3 and 4

#### Scenario: Shared settings
- **WHEN** two Profiles have slot 2 and the user changes a Leakshield setting of that slot
- **THEN** both Profiles use the new value

### Requirement: Change the slot of a Profile
The user SHALL be able to change the slot of a Profile, except the Default Profile. A slot outside 1 to 4 SHALL be refused. When the Profile is active, the daemon SHALL apply the new slot as an activation does.

#### Scenario: Change the slot
- **WHEN** the user changes the slot of a Profile from 3 to 4
- **THEN** the Profile uses the Leakshield settings of slot 4, and the settings of slot 3 do not change

#### Scenario: Slot out of range
- **WHEN** a client sets the slot of a Profile to 5
- **THEN** the daemon refuses with `InvalidArgument`

#### Scenario: Change the slot of the Default Profile
- **WHEN** a client sets the slot of the Default Profile
- **THEN** the daemon refuses with `InvalidArgument`

#### Scenario: Change the slot of the active Profile
- **WHEN** the user changes the slot of the active Profile and the device reports another number
- **THEN** the daemon applies the new slot as an activation does

### Requirement: Default Profile
A Default Profile SHALL always exist and SHALL NOT be deletable. It SHALL be active when no Rule group wins and no manual activation exists. A change to a Curve that the user makes while it is active SHALL go into it.

#### Scenario: No group wins
- **WHEN** no Rule group has held for its Rule delay and no manual activation exists
- **THEN** the Default Profile is active

#### Scenario: Delete the Default Profile
- **WHEN** a client asks to delete the Default Profile
- **THEN** the daemon refuses

#### Scenario: Change with the Default Profile active
- **WHEN** the user changes a Curve while the Default Profile is active
- **THEN** the Default Profile holds the new Curve

### Requirement: Activate as one unit
Activating a Profile SHALL apply its Curves and the Leakshield settings of its slot as one unit.

#### Scenario: Activation
- **WHEN** the user activates a Profile
- **THEN** the Curves of the Profile and the Leakshield values of its slot become active

### Requirement: Partial switch
When the Leakshield part of a Profile switch fails with a write error, the Curves SHALL stay applied. The API SHALL report the state "Curves applied, Leakshield failed" and offer the restore of the backup.

#### Scenario: Leakshield write fails
- **WHEN** a Profile switch applies the Curves and the write of the report 3 fails
- **THEN** the Profile state is "Curves applied, Leakshield failed" and the restore is offered

### Requirement: Write only when the slot differs
A Profile switch SHALL write the stored whole report 3 of the slot of the Profile to a Leakshield, as stored, only when the slot number differs from the number that the device reports in report 1 `active_profile`. It SHALL NOT compare bytes and SHALL NOT read the report back. A switch between two Profiles with the same slot SHALL write only the Curves.

#### Scenario: Device already has the number
- **WHEN** the user activates a Profile and the device already reports the slot number of that Profile
- **THEN** the daemon writes nothing to the Leakshield

#### Scenario: Different number
- **WHEN** the user activates a Profile and the device reports another number
- **THEN** the daemon writes the whole stored report 3 of the slot as stored, and does not read it back

#### Scenario: Two Profiles on one slot
- **WHEN** the user switches from a Profile with slot 2 to another Profile with slot 2
- **THEN** the daemon applies the Curves of the second Profile and writes nothing to the Leakshield

### Requirement: Write in any Mode
A Profile switch SHALL write the Leakshield also while an Action runs and while the Mode is RELEASE.

#### Scenario: Action running
- **WHEN** a Test runs and a Rule group activates a Profile with another slot
- **THEN** the Curves apply and the daemon writes the stored report 3 to the Leakshield

#### Scenario: Mode RELEASE
- **WHEN** the Mode is RELEASE and a Profile with another slot and Mode SHIELD is activated
- **THEN** the daemon writes the stored report 3

### Requirement: Write only to a matching device
A Profile switch SHALL NOT write a Leakshield whose firmware is unknown, whose firmware differs from the firmware of the stored report 3 of the slot, or whose serial differs from the serial of that report 3.

#### Scenario: Different firmware
- **WHEN** the stored settings of the slot come from another firmware version than the device runs
- **THEN** the daemon writes no Leakshield setting and reports "Curves applied, Leakshield failed"

#### Scenario: Other serial
- **WHEN** the stored report 3 of the slot comes from a Leakshield with another serial than the device
- **THEN** the daemon writes no Leakshield setting and reports "Curves applied, Leakshield failed"

### Requirement: Leakshield write spacing
The daemon SHALL write a Profile switch that needs a Leakshield write at once when no write waits and the last profile write is older than 2 seconds. Otherwise it SHALL keep only the newest request and write it 1.5 seconds after the last profile write. It SHALL send the save-settings command 4 seconds after the last report 3 write.

#### Scenario: Write at once
- **WHEN** a Profile switch needs a write, no write waits and the last profile write is 10 seconds old
- **THEN** the daemon writes the report 3 at once

#### Scenario: Newest request wins
- **WHEN** three Profile switches need a write within 2 seconds after a profile write
- **THEN** the daemon writes only the newest one, 1.5 seconds after the last profile write

#### Scenario: Save-settings follows
- **WHEN** the daemon writes a report 3 for a Profile switch
- **THEN** it sends one save-settings command 4 seconds after that write

### Requirement: Leakshield write order
Profile requests SHALL reach the Leakshield in the order of the activations. A newer request SHALL cancel the request that waits, also when the newer request fails or needs no write. A request that waits SHALL write the slot as it is when the write goes out.

#### Scenario: Close activations end on the active slot
- **WHEN** the user activates Profile A and then Profile B within 2 seconds
- **THEN** the Leakshield ends on the slot of Profile B

#### Scenario: Newer request fails
- **WHEN** a request waits for its write and a newer request fails because its slot has no stored report 3
- **THEN** the daemon drops the request that waits and writes nothing more

#### Scenario: Slot changes while the request waits
- **WHEN** the user changes a Leakshield setting while a Profile write waits
- **THEN** the write that goes out carries the change, and the device and the slot hold the same value

### Requirement: Overwrite external changes
Activating a Profile whose slot number differs from the number of the device SHALL write the settings of the slot over a Leakshield setting that another tool changed.

#### Scenario: External change overwritten
- **WHEN** the device button changed a field and the user activates a Profile with another slot
- **THEN** the field takes the value of the slot

#### Scenario: External change kept
- **WHEN** the device button changed a field and the user activates a Profile whose slot number the device already reports
- **THEN** the field keeps the value that the button set

### Requirement: Rule groups
A Rule group SHALL hold an ordered list of Profile rules and a target Profile. A group SHALL hold when it is enabled and all its rules hold. The daemon SHALL evaluate the groups in list order every tick. The first group that holds, and has held for the Rule delay, SHALL activate its Profile. When no group holds, the Default Profile SHALL be active. The daemon SHALL NOT offer hotkeys as a rule kind, because Wayland has no standard global shortcut.

#### Scenario: All rules hold
- **WHEN** a group has a process rule and a time rule, and both hold
- **THEN** the group holds

#### Scenario: One rule does not hold
- **WHEN** a group has a process rule that holds and a time rule that does not hold
- **THEN** the group does not hold

#### Scenario: Two groups hold
- **WHEN** the second and the fourth group in the list have both held for the Rule delay
- **THEN** the second group wins and activates its Profile

#### Scenario: Group without rules
- **WHEN** a group has no rule
- **THEN** the group never holds

#### Scenario: Disabled group
- **WHEN** all rules of a disabled group hold
- **THEN** the group does not hold

### Requirement: Rule kinds
The daemon SHALL evaluate four rule kinds as `docs/api.md` defines them, in section Rule groups: a sensor comparison with a hold time, a process list, weekdays with a time window, and an interval timer.

#### Scenario: Sensor comparison
- **WHEN** a sensor rule has the comparison `gt` and the value 50, and the sensor reads 55
- **THEN** the rule holds after its hold time

#### Scenario: Sensor inside a range
- **WHEN** a sensor rule has the comparison `in_range` with the values 30 and 40, and the sensor reads 35
- **THEN** the rule holds after its hold time

#### Scenario: Sensor hold time
- **WHEN** the sensor of a rule with the hold time 60 s meets the comparison for 20 s and then leaves it
- **THEN** the rule does not hold

#### Scenario: Sensor without a value
- **WHEN** the sensor of a sensor rule has no value
- **THEN** the rule does not hold

#### Scenario: Process name
- **WHEN** a process rule lists the name `game` and a process runs with `comm` `game` or an executable `/opt/x/game`
- **THEN** the rule holds, and a process `Game` also matches, because the match ignores case

#### Scenario: Command line match
- **WHEN** a process rule lists `Cyberpunk2077.exe` with `match_on` `cmdline`, and a Proton game runs whose executable is the Wine loader
- **THEN** the rule holds, because the command line of the process contains the name

#### Scenario: Process list modes
- **WHEN** a process rule lists two names, and only one of them runs
- **THEN** the match `any` holds, the match `all` does not hold and the match `none` does not hold

#### Scenario: Time range across midnight
- **WHEN** a time rule has the days Friday to Saturday, start 22:00 and end 02:00
- **THEN** the rule holds on Saturday at 01:00, because the range belongs to Friday

#### Scenario: Interval timer
- **WHEN** an interval rule has the interval 600 s, the start delay 60 s and the duration 120 s
- **THEN** the rule holds from 60 s to 180 s after the start of each period of 600 s

### Requirement: Edge-triggered activation
A Rule group SHALL activate its Profile only when it becomes the winning group. It SHALL NOT activate the Profile again on later ticks while it stays the winner.

#### Scenario: Group stays true
- **WHEN** a group has won and holds for 100 ticks
- **THEN** the daemon activates its Profile once

#### Scenario: Setting changed while the group holds
- **WHEN** a group has won and the device button changes a Leakshield setting
- **THEN** the daemon does not write the settings of the slot over the change

### Requirement: Rule delay
Each Rule group SHALL have its own Rule delay timer. The winning group SHALL be the first group in list order that has held for the Rule delay. The Rule delay SHALL apply to every group and default to 30 seconds. An edit of the winning group SHALL keep it active without a new Rule delay while it still holds. The timer of an edited group SHALL restart only when the group stops holding. An edit SHALL keep the hold time and the interval phase of each rule that it does not change.

#### Scenario: Group holds
- **WHEN** a group holds for 30 seconds with the default Rule delay
- **THEN** the group wins and activates its Profile

#### Scenario: Rule delay changed
- **WHEN** the user sets the Rule delay to 10 seconds and a group holds for 10 seconds
- **THEN** the group wins and activates its Profile

#### Scenario: Group stops early
- **WHEN** the group stops holding after 10 seconds
- **THEN** the group does not activate its Profile and its timer resets

#### Scenario: Edit of the winning group
- **WHEN** the user edits the winning group and the edited group still holds
- **THEN** the group stays the winner, its Profile stays active and its Rule delay does not start again

#### Scenario: Edit stops the winning group
- **WHEN** the user edits the winning group so that it does not hold
- **THEN** its timer resets and the group needs the whole Rule delay when it holds again

#### Scenario: Edit keeps an unchanged rule
- **WHEN** the user edits a group and keeps a sensor rule that has met its comparison for 40 seconds of its 60 seconds hold time
- **THEN** that rule holds after 20 more seconds, and a new or changed rule starts its hold time again

#### Scenario: Later group is ready first
- **WHEN** the third group has held for 30 seconds and the first group has held for 10 seconds
- **THEN** the third group wins, and the first group wins when its own 30 seconds end

### Requirement: Profile dwell
A Profile that a Rule group activated SHALL stay active for the Profile dwell. The Profile dwell SHALL be a setting that applies to every group. Its default SHALL be 5 minutes.

#### Scenario: Profile dwell changed
- **WHEN** the user sets the Profile dwell to 1 minute and another group asks for a switch 2 minutes after a group activation
- **THEN** the Profile of that group becomes active

#### Scenario: Group changes inside dwell
- **WHEN** another group asks for a switch 2 minutes after a group activation
- **THEN** the active Profile stays until the dwell ends

#### Scenario: No group wins after the dwell
- **WHEN** the winning group stops holding 2 minutes after its activation and no other group wins
- **THEN** its Profile stays active until the dwell ends, and then the Default Profile becomes active

### Requirement: Immediate switching
When the user turns on Immediate switching, the daemon SHALL apply Rule group changes at once, without the Rule delay and the Profile dwell.

#### Scenario: Immediate switching on
- **WHEN** Immediate switching is on and a group starts to hold
- **THEN** the group activates its Profile in the same tick

### Requirement: Manual activation wins
A Profile that the user activated by hand SHALL become active at once, also during the Profile dwell. It SHALL stay active until the user returns to automatic.

#### Scenario: Rule during manual activation
- **WHEN** a Rule group starts to hold while the user holds a manual activation
- **THEN** the daemon does not switch Profile

#### Scenario: Last command wins
- **WHEN** the user activates a Profile by hand and returns to automatic within one tick
- **THEN** the Rule groups decide, and the last command in arrival order decides when the order is the other way round

#### Scenario: Manual activation during the dwell
- **WHEN** the user activates a Profile by hand 2 minutes after a group activation
- **THEN** the Profile becomes active in the same tick

### Requirement: Edits go into the active Profile and its slot
A change to a Curve SHALL go into the active Profile. A change to a Leakshield setting that the user makes in nicecooling SHALL go into the slot that the `device_profile` byte of the read-back names, which is the slot that the device runs and, after a switch that worked, the slot of the active Profile, and so into every Profile with that slot.

#### Scenario: Curve edit
- **WHEN** the user changes a Curve while Profile A is active
- **THEN** Profile A holds the new Curve and no other Profile changes

#### Scenario: Leakshield edit after a failed switch
- **WHEN** the switch to Profile A (slot 2) failed, the device runs slot 1, and the user changes a Leakshield setting
- **THEN** slot 1 holds the new value, and the stored settings of slot 2 and their edited mark stay as they were

#### Scenario: Leakshield edit with a shared slot
- **WHEN** Profile A and Profile B have slot 2, A is active, and the user changes a Leakshield setting
- **THEN** slot 2 holds the new value and Profile B uses it when activated

### Requirement: Profiles with critical values
Saving a Profile that contains Mode RELEASE SHALL use the action `write`, as any other save, and SHALL NOT need admin authorization.

#### Scenario: Save with RELEASE
- **WHEN** a user with write authorization saves a Profile with Mode RELEASE
- **THEN** the daemon saves it without an admin password

### Requirement: Profile dwell end
The daemon SHALL expose the end time of the running Profile dwell, and 0 when no dwell runs, and SHALL update it when the dwell, the cause or the switching options change.

#### Scenario: Dwell runs
- **WHEN** a Rule group activates a Profile with a dwell of 300 s
- **THEN** `DwellUntil` is the time of the switch plus 300 s, and 0 after the dwell ends

#### Scenario: No dwell
- **WHEN** Immediate switching is on, or the activation is manual
- **THEN** `DwellUntil` is 0

#### Scenario: Daemon restart
- **WHEN** the daemon restarts while a Rule group holds
- **THEN** `DwellUntil` counts from the daemon start

### Requirement: Stored settings of another slot
The daemon SHALL read and write the stored Leakshield settings of a slot, in the write worker of the Leakshield, with the validation of `SetFields` against the firmware of the slot, and without device traffic. A write SHALL refuse the active slot.

#### Scenario: Apply to another slot
- **WHEN** the user calls `SetStoredFields` for slot 3 while slot 1 is active
- **THEN** the stored report 3 of slot 3 holds the new values with a valid CRC, and the device gets no write

#### Scenario: Active slot
- **WHEN** the user calls `SetStoredFields` for the slot of report 1 `active_profile`
- **THEN** the daemon returns `InvalidArgument`

#### Scenario: Slot without stored settings
- **WHEN** the slot has no stored report 3
- **THEN** `GetStoredFields` and `SetStoredFields` return `NotFound`

### Requirement: Verified write of an edited slot
The first Profile activation of a slot that `SetStoredFields` changed SHALL use the verified write path (backup, write, read-back). A good read-back SHALL clear the mark.

#### Scenario: Activation of an edited slot
- **WHEN** a Profile with an edited slot becomes active
- **THEN** the daemon reads report 3 back after the write and compares it with the stored report outside the volatile mask

#### Scenario: Read-back differs
- **WHEN** the read-back differs
- **THEN** the activation reports `leakshield-failed`, `FailedWriteBackup` holds the backup id, and the slot stays marked
