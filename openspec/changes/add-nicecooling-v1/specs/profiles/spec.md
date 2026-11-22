# Profiles

## Purpose

Defines Profiles, the Default Profile, the Profile rules that activate them, the timing rules that stop fast switching, and the Maintenance pause that protects the Leakshield during maintenance.

## ADDED Requirements

### Requirement: Profile content
A Profile SHALL hold a set of Curves and the whole settings report of each Leakshield as the device produced it, with binary assets such as display logos. The daemon SHALL store each Profile in one place.

#### Scenario: Save a Profile
- **WHEN** the user saves a Profile with a display logo
- **THEN** activating it later writes the logo and the stored settings

### Requirement: Default Profile
A Default Profile SHALL always exist and SHALL NOT be deletable. It SHALL be active when no Profile rule wins and no manual activation exists. A change that the user makes while it is active SHALL go into it.

#### Scenario: No rule wins
- **WHEN** no rule condition has held for its Rule delay and no manual activation exists
- **THEN** the Default Profile is active

#### Scenario: Delete the Default Profile
- **WHEN** a client asks to delete the Default Profile
- **THEN** the daemon refuses

#### Scenario: Change with the Default Profile active
- **WHEN** the user changes a Curve while the Default Profile is active
- **THEN** the Default Profile holds the new Curve

### Requirement: Activate as one unit
Activating a Profile SHALL apply its Curves and its Leakshield snapshot as one unit.

#### Scenario: Activation
- **WHEN** the user activates a Profile
- **THEN** the Curves and the Leakshield values of the Profile become active

### Requirement: Partial switch
When the Leakshield part of a Profile switch fails its read-back, the Curves SHALL stay applied. The API SHALL report the state "Curves applied, Leakshield failed" and offer the restore of the backup.

#### Scenario: Leakshield write fails
- **WHEN** a Profile switch applies the Curves and the Leakshield read-back fails
- **THEN** the Profile state is "Curves applied, Leakshield failed" and the restore is offered

### Requirement: Write only on a difference
A Profile switch SHALL write the stored Leakshield settings only when they differ from the device settings outside the volatile mask. It SHALL keep the volatile bytes of the device. It SHALL NOT write them when the Profile and the device have different firmware.

#### Scenario: Same Leakshield values
- **WHEN** two Profiles hold the same Leakshield settings
- **THEN** a switch between them writes nothing to the Leakshield

#### Scenario: Different firmware
- **WHEN** the stored settings come from another firmware version than the device runs
- **THEN** the daemon writes no Leakshield setting and reports "Curves applied, Leakshield failed"

### Requirement: Overwrite external changes
Activating a Profile SHALL write the Profile snapshot over a Leakshield setting that another tool changed.

#### Scenario: External change overwritten
- **WHEN** the device button changed a field and the user activates a Profile
- **THEN** the field takes the value of the Profile

### Requirement: Profile rules
A Profile rule SHALL activate a Profile on a condition: a running process, a time or a sensor threshold. The daemon SHALL evaluate the rules in list order every tick.

#### Scenario: Process rule
- **WHEN** a process named by a rule starts
- **THEN** the rule condition becomes true within 2 seconds

#### Scenario: Sensor threshold
- **WHEN** a sensor rises above the threshold of a rule
- **THEN** the rule condition becomes true

#### Scenario: Two rules true
- **WHEN** the conditions of the second and the fourth rule in the list have both held for their Rule delay
- **THEN** the second rule wins and activates its Profile

### Requirement: Rule semantics
The daemon SHALL evaluate the three rule types as `docs/api.md` defines them, in section Profile rule parameters.

#### Scenario: Process name
- **WHEN** a rule matches the name `game` and a process runs with `comm` `game` or an executable `/opt/x/game`
- **THEN** the rule condition is true, and a process `Game` does not match

#### Scenario: Process command line
- **WHEN** a rule matches the command line with the pattern `Render` and a process has `--mode render` in its command line
- **THEN** the rule condition is true

#### Scenario: Time range across midnight
- **WHEN** a time rule has the days Friday to Saturday, start 22:00 and end 02:00
- **THEN** the condition is true on Saturday at 01:00, because the range belongs to Friday

#### Scenario: Threshold above
- **WHEN** a threshold rule has the direction `above` and the value 50, and the sensor reads 55
- **THEN** the condition is true

#### Scenario: Threshold below
- **WHEN** a threshold rule has the direction `below` and the value 50, and the sensor reads 45
- **THEN** the condition is true

#### Scenario: Threshold debounce
- **WHEN** the sensor of a threshold rule crosses the value for 10 seconds and returns
- **THEN** the rule does not activate its Profile, because the Rule delay is the debounce

### Requirement: Edge-triggered activation
A Profile rule SHALL activate its Profile only when it becomes the winning rule. It SHALL NOT activate the Profile again on later ticks while it stays the winner.

#### Scenario: Rule stays true
- **WHEN** a rule has won and its condition stays true for 100 ticks
- **THEN** the daemon activates its Profile once

#### Scenario: Setting changed while the rule is true
- **WHEN** a rule has won and the device button changes a Leakshield setting
- **THEN** the daemon does not write the Profile snapshot over the change

### Requirement: Rule delay
Each Profile rule SHALL have its own Rule delay timer. The winning rule SHALL be the first rule in list order whose condition has held for the Rule delay. The Rule delay SHALL be a setting that applies to every rule. Its default SHALL be 30 seconds.

#### Scenario: Condition lasts
- **WHEN** a condition stays true for 30 seconds with the default Rule delay
- **THEN** the rule wins and activates its Profile

#### Scenario: Rule delay changed
- **WHEN** the user sets the Rule delay to 10 seconds and a condition stays true for 10 seconds
- **THEN** the rule wins and activates its Profile

#### Scenario: Condition ends early
- **WHEN** the condition becomes false after 10 seconds
- **THEN** the rule does not activate its Profile and its timer resets

#### Scenario: Later rule is ready first
- **WHEN** the third rule has held for 30 seconds and the first rule has held for 10 seconds
- **THEN** the third rule wins, and the first rule wins when its own 30 seconds end

### Requirement: Profile dwell
A Profile that a Profile rule activated SHALL stay active for the Profile dwell. The Profile dwell SHALL be a setting that applies to every rule. Its default SHALL be 5 minutes.

#### Scenario: Profile dwell changed
- **WHEN** the user sets the Profile dwell to 1 minute and another rule asks for a switch 2 minutes after a rule activation
- **THEN** the Profile of that rule becomes active

#### Scenario: Rule changes inside dwell
- **WHEN** another rule asks for a switch 2 minutes after a rule activation
- **THEN** the active Profile stays until the dwell ends

#### Scenario: No rule wins after the dwell
- **WHEN** the condition of the winning rule becomes false 2 minutes after its activation and no other rule wins
- **THEN** its Profile stays active until the dwell ends, and then the Default Profile becomes active

### Requirement: Immediate switching
When the user turns on Immediate switching, the daemon SHALL apply Profile rule changes at once, without the Rule delay and the Profile dwell.

#### Scenario: Immediate switching on
- **WHEN** Immediate switching is on and a condition becomes true
- **THEN** the rule activates its Profile in the same tick

### Requirement: Manual activation wins
A Profile that the user activated by hand SHALL become active at once, also during the Profile dwell. It SHALL stay active until the user returns to automatic.

#### Scenario: Rule during manual activation
- **WHEN** a rule condition becomes true while the user holds a manual activation
- **THEN** the daemon does not switch Profile

#### Scenario: Manual activation during the dwell
- **WHEN** the user activates a Profile by hand 2 minutes after a rule activation
- **THEN** the Profile becomes active in the same tick

### Requirement: Maintenance pause
While an Action runs, or while the Mode is RELEASE and no Profile activation wrote it, Profile rules SHALL NOT write the Leakshield. Any RELEASE that a Profile activation did not write SHALL open the pause. Curves SHALL still follow the rules.

#### Scenario: RELEASE from the device button
- **WHEN** the user sets the Mode RELEASE with the device button
- **THEN** the Maintenance pause opens and no rule writes the Leakshield

#### Scenario: RELEASE from a Profile
- **WHEN** a Profile activation writes the Mode RELEASE
- **THEN** the Maintenance pause does not open

#### Scenario: Time rule during Test
- **WHEN** a Test runs and a time rule activates a Profile with Mode SHIELD
- **THEN** the Curves of the Profile apply and the Leakshield receives no write

#### Scenario: Pause ends
- **WHEN** a rule became the winner during the pause and the Action then ends
- **THEN** the daemon does not write the Leakshield part of that Profile, and the next activation of a Profile writes the Leakshield again

### Requirement: Profiles with critical values
Saving a Profile that contains Mode RELEASE SHALL NOT need admin authorization.

#### Scenario: Save with RELEASE
- **WHEN** a user with write authorization saves a Profile with Mode RELEASE
- **THEN** the daemon saves it without an admin password
