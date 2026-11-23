# Alarms

## Purpose

Defines the sources of Warnings and Alarms, the alarm response of each, the cancellable poweroff and its time bound, and the coordination with the alarm outputs of the Leakshield itself.

## ADDED Requirements

### Requirement: Leakshield Warnings and Alarms
The daemon SHALL decode Warnings and Alarms from the Leakshield and treat each Alarm as a source of nicecooling alarm handling.

#### Scenario: Leak Alarm
- **WHEN** the Leakshield raises a leak Alarm
- **THEN** the daemon raises the matching Alarm and runs its configured alarm response

### Requirement: Pump-stop Alarm
The daemon SHALL raise a pump-stop Alarm for a Loop when any of its Pump channels reads 0 rpm for 5 seconds while its Duty is above zero. For a Channel on its Hardware curve, the Duty is the current `pwmN` value.

#### Scenario: Pump stops
- **WHEN** the Pump channel has Duty 60% and reads 0 rpm for 5 seconds
- **THEN** the daemon raises the pump-stop Alarm

#### Scenario: Pump off by choice
- **WHEN** the Pump channel has Duty 0% and reads 0 rpm
- **THEN** the daemon raises no Alarm

#### Scenario: One of two Pump channels stops
- **WHEN** a Loop has two Pump channels and one reads 0 rpm for 5 seconds with Duty above zero
- **THEN** the daemon raises the pump-stop Alarm

#### Scenario: Pump on its Hardware curve
- **WHEN** a Pump channel on its Hardware curve reads 0 rpm for 5 seconds while its `pwmN` value is above zero
- **THEN** the daemon raises the pump-stop Alarm

### Requirement: Pump stop sets full Duty
On a pump-stop Alarm, the daemon SHALL set every Channel to 100%, Channels on their Hardware curve included. This override SHALL bypass the Duty step limit.

#### Scenario: Full Duty
- **WHEN** the pump-stop Alarm is raised
- **THEN** every Channel has Duty 100% in the same tick

#### Scenario: Hardware-curve Channel
- **WHEN** the pump-stop Alarm is raised and a Channel has no Curve and no fixed Duty
- **THEN** the daemon sets that Channel to 100%

#### Scenario: Pump runs again
- **WHEN** every Pump channel of the Loop reads more than 0 rpm for 10 seconds in a row
- **THEN** the pump-stop Alarm closes and the Curves control the Channels again

#### Scenario: Channels return to the Board
- **WHEN** the pump-stop Alarm closes
- **THEN** each Channel without a Curve or a fixed Duty gets its BIOS snapshot attributes back

### Requirement: Leakshield unreachable Alarm
The daemon SHALL raise the Alarm "Leakshield unreachable" when report 1 is missing, or the Feed message fails, for 10 seconds. The default alarm response SHALL be notify.

#### Scenario: Device disconnected
- **WHEN** the Leakshield sends no report 1 for 10 seconds
- **THEN** the daemon raises the Alarm "Leakshield unreachable" and notifies

#### Scenario: Feed message fails
- **WHEN** the Feed message fails for 10 seconds
- **THEN** the daemon raises the Alarm "Leakshield unreachable"

#### Scenario: Leakshield answers again
- **WHEN** the Leakshield sends report 1 and accepts the Feed message for 10 seconds in a row
- **THEN** the Alarm "Leakshield unreachable" closes

#### Scenario: Short gap
- **WHEN** report 1 is missing for 4 seconds and then returns
- **THEN** the daemon raises no Alarm

### Requirement: Alarm response per Warning and Alarm
The user SHALL configure one alarm response for each Warning and each Alarm, in one table: none, notify, or poweroff after a delay.

#### Scenario: Notify only
- **WHEN** an Alarm has the alarm response notify
- **THEN** the daemon emits the Alarm signal and does not power off the PC

### Requirement: Default alarm responses
Without user configuration, the large leak Alarm and the pump-stop Alarm SHALL power off after a delay of 30 seconds. Every other Alarm and every Warning SHALL notify.

#### Scenario: Large leak with defaults
- **WHEN** the Leakshield raises the large leak Alarm and the user changed no alarm response
- **THEN** the daemon notifies and powers off the PC 30 seconds later unless the user cancels

#### Scenario: Warning with defaults
- **WHEN** the Leakshield raises a Warning and the user changed no alarm response
- **THEN** the daemon notifies and does not power off the PC

### Requirement: Cancellable poweroff delay
When an Alarm has the alarm response poweroff, the daemon SHALL wait for the configured delay before it powers off the PC. The user SHALL be able to cancel during the delay. The app of any session SHALL be able to cancel.

#### Scenario: Delay expires
- **WHEN** the delay ends and nobody cancelled
- **THEN** the daemon asks the system to power off

#### Scenario: User cancels
- **WHEN** the user cancels during the delay
- **THEN** the PC does not power off

#### Scenario: Cancel from another session
- **WHEN** a countdown runs and the app of another session calls CancelPoweroff
- **THEN** the pending poweroff is cancelled

#### Scenario: Daemon restarts during the countdown
- **WHEN** the daemon restarts during a countdown and the Alarm is still active
- **THEN** the countdown starts again with the full delay

### Requirement: Poweroff call
At the end of the delay, the daemon SHALL call logind `PowerOffWithFlags` with the flag `SD_LOGIND_SKIP_INHIBITORS` (0x10). When the call fails, the daemon SHALL emit `PoweroffFailed` and retry every 5 seconds while the Alarm is active.

#### Scenario: Block inhibitor held
- **WHEN** a process holds a block inhibitor and the delay ends
- **THEN** the daemon calls `PowerOffWithFlags` with the flag 0x10, and logind does not wait for that inhibitor

#### Scenario: Call fails
- **WHEN** the logind call returns an error
- **THEN** the daemon emits `PoweroffFailed` and calls again after 5 seconds

#### Scenario: logind does not answer
- **WHEN** the logind call has no answer after 10 seconds
- **THEN** the daemon counts the call as failed, emits `PoweroffFailed` and calls again after 5 seconds

#### Scenario: Cancel after the call started
- **WHEN** the user calls CancelPoweroff while the logind call is under way
- **THEN** the call fails with `PoweroffStarted` and the daemon emits no `PoweroffCancelled`

### Requirement: Poweroff time bound
The RPM SHALL install a drop-in for `poweroff.target` with `JobTimeoutSec=55s` and `JobTimeoutAction=poweroff-force`. Delay inhibitors wait up to `InhibitDelayMaxSec`, which is 5 seconds by default. From the end of the alarm delay to the start of the forced poweroff, at most 60 seconds SHALL pass with the default `InhibitDelayMaxSec`. The time to power off is 60 seconds as a goal and not a bound: `systemd-shutdown` waits for the processes after SIGTERM and SIGKILL.

#### Scenario: Shutdown stalls
- **WHEN** an orderly shutdown takes more than 55 seconds after logind queues the job
- **THEN** systemd forces the poweroff

#### Scenario: Drop-in values
- **WHEN** the RPM is installed
- **THEN** `systemctl show poweroff.target` shows `JobTimeoutUSec=55s` and `JobTimeoutAction=poweroff-force`

### Requirement: Alarm announcement
The daemon SHALL announce each Alarm on the D-Bus API. The desktop app SHALL show the notification, with a cancel button while a poweroff is pending.

#### Scenario: Notification with cancel
- **WHEN** an Alarm with a pending poweroff is raised and the app runs
- **THEN** the user sees a notification with a cancel button

#### Scenario: App not running
- **WHEN** no session runs the app
- **THEN** the daemon still runs the configured alarm response, and the notification does not appear

### Requirement: Device alarm settings
nicecooling SHALL propose these device settings through the API method `GetDeviceResponseProposals`: the key-sequence Device response off on every alarm, and the signal output only as a long press on the system Alarm. It SHALL warn when a device setting overlaps with its own alarm responses. The overlap warnings are `key-sequence-on`, and `double-poweroff` for a signal output on a Leakshield Alarm whose alarm response is poweroff. It SHALL NOT write the proposals on its own.

#### Scenario: Key-sequence output on
- **WHEN** the device key-sequence output is on
- **THEN** nicecooling warns `key-sequence-on`: the Leakshield types the sequence into the focused window

#### Scenario: Signal output and poweroff
- **WHEN** the signal output is set on a Leakshield Alarm whose alarm response is poweroff
- **THEN** nicecooling warns `double-poweroff`

#### Scenario: Proposals listed
- **WHEN** a client calls `GetDeviceResponseProposals`
- **THEN** it gets the proposals and the overlap warnings, and the daemon writes nothing

#### Scenario: Proposal accepted
- **WHEN** the user accepts a proposed device setting
- **THEN** nicecooling writes it through the normal write path

### Requirement: Leaks handled by the daemon
The daemon alarm responses SHALL handle leak Alarms. The device signal output SHALL stay as a backup for the system Alarm only.

#### Scenario: Linux hung
- **WHEN** Linux hangs and the daemon cannot act
- **THEN** the device signal output of the system Alarm is the only remaining protection
