# Graph actions

## Purpose

Defines the action node of a Graph: its timing state machine, its eight kinds with their parameters, which process runs each kind, the request and outcome exchange between the daemon and the app, the countdown of the power kinds, the message placeholders, the action log and the SMTP account.

## ADDED Requirements

### Requirement: Action node parameters
An action node SHALL have one input pin, one output pin, a kind, a hold time, "repeat every" and a minimum interval. The hold time SHALL be 0 to 3600 s, default 0 s. "Repeat every" SHALL be off or 1 to 86400 s, default 60 s when on. The minimum interval SHALL be 0 to 86400 s, default 0 s. The output pin SHALL give 1 in the state "run" and 0 in the state "not run".

#### Scenario: Defaults
- **WHEN** the user adds an action node
- **THEN** the hold time is 0 s, repeat is off with 60 s ready, and the minimum interval is 0 s

#### Scenario: Hold time above the limit
- **WHEN** a Graph write has a hold time of 3601 s
- **THEN** the daemon refuses it with `InvalidArgument`

### Requirement: Action state machine
The daemon SHALL evaluate each action node once per tick. An input that is false or has no value SHALL clear the trigger time and set the state "not run". A rise from false to true SHALL set the trigger time to now. In the state "not run", when the trigger time plus the hold time is not after now, the node SHALL run its action and set the state "run".

#### Scenario: Hold time
- **WHEN** the hold time is 10 s and the input rises at second 100 and stays true
- **THEN** the action runs once at second 110

#### Scenario: Input falls inside the hold time
- **WHEN** the hold time is 10 s and the input is true from second 100 to second 105 and then false
- **THEN** the action does not run

#### Scenario: No action on the falling edge
- **WHEN** the input falls after a run
- **THEN** the action does not run and the state returns to "not run"

#### Scenario: No value counts as false
- **WHEN** the input has no value for one tick
- **THEN** the trigger time is forgotten and a later true input starts the hold time again

### Requirement: Repeat
In the state "run" with repeat on, the node SHALL run its action again when the last run plus the repeat interval is not after now and the input is still true. Without repeat the node SHALL run once for each rise.

#### Scenario: Repeat
- **WHEN** repeat is on at 60 s and the action ran at second 110 while the input stays true
- **THEN** the action runs again at seconds 170 and 230

#### Scenario: No repeat
- **WHEN** repeat is off and the input stays true for 1 hour after the run
- **THEN** the action runs once

### Requirement: Minimum interval
A run that would start before the last run plus the minimum interval SHALL be dropped. A dropped run SHALL leave the state at "run" and SHALL not write a log row. A dropped first run SHALL not be tried again unless repeat is on.

#### Scenario: Second raise inside the interval
- **WHEN** the minimum interval is 300 s, the action ran at second 100, the input falls and rises at second 200
- **THEN** the run at second 200 is dropped

#### Scenario: Interval over
- **WHEN** the same Graph rises again at second 420
- **THEN** the action runs

#### Scenario: Dropped first run
- **WHEN** a first run is dropped and repeat is off and the input stays true
- **THEN** the action does not run until the input falls and rises again

### Requirement: Test run
"Test" on an action node SHALL run the action once at once, with the parameters of the draft, and SHALL ignore the minimum interval. It SHALL not change the state of the node. Test SHALL use the polkit action `io.github.cappyt.NiceCooling1.write`.

#### Scenario: Test inside the interval
- **WHEN** the action ran 10 s ago with a minimum interval of 300 s and the user presses "Test"
- **THEN** the action runs again

#### Scenario: Test of a draft
- **WHEN** the user presses "Test" on a node that is not applied yet
- **THEN** the action runs with the parameters in the editor

### Requirement: Start delay
For 10 s after the start of the daemon no action SHALL run. In that time the action nodes SHALL see their input as false. Sensor nodes SHALL evaluate from the first tick.

#### Scenario: Condition true at boot
- **WHEN** the input of an action node with hold time 0 s is true from the first tick
- **THEN** the action runs at second 10 after the start and not before

#### Scenario: Sensor at boot
- **WHEN** a sensor Graph has a value in the first tick
- **THEN** its sensor has that value in the first tick

### Requirement: Action kinds and process
The action kinds SHALL be notification, sound, command, e-mail, MQTT message, poweroff, reboot and suspend. The app SHALL run notification, sound and command. The daemon SHALL run e-mail, MQTT message, poweroff, reboot and suspend. No kind SHALL ask for a password.

#### Scenario: Command runs in the app
- **WHEN** a command action fires
- **THEN** the daemon asks the app and the app starts the program in the user session

#### Scenario: E-mail runs in the daemon
- **WHEN** an e-mail action fires with no user logged in
- **THEN** the daemon sends the e-mail

### Requirement: Notification action
A notification action SHALL have a title of 1 to 64 characters, a text of up to 1024 characters and an urgency of low, normal or critical, default normal. The app SHALL show it as a desktop notification.

#### Scenario: Notification
- **WHEN** the action fires with the title "Water hot" and the urgency normal
- **THEN** the app shows a notification with that title

#### Scenario: Empty title
- **WHEN** a Graph write has an empty title
- **THEN** the daemon refuses it with `InvalidArgument`

### Requirement: Sound action
A sound action SHALL have one name from the sound theme of the desktop. The app SHALL play it as the sound hint of a notification that carries the name of the Graph as title.

#### Scenario: Sound
- **WHEN** the action fires with the sound `dialog-warning`
- **THEN** the app sends a notification with the sound name `dialog-warning`

#### Scenario: Empty sound name
- **WHEN** a Graph write has an empty sound name
- **THEN** the daemon refuses it with `InvalidArgument`

### Requirement: Command action
A command action SHALL have an absolute program path, a list of arguments and a time limit of 1 to 3600 s, default 30 s. The app SHALL start the program in the user session with no shell, SHALL discard its output and SHALL kill it, with its process group, at the time limit. The app SHALL set the environment variables with the Graph id, the Graph name, the run id and the value of each node.

#### Scenario: Run with arguments
- **WHEN** the path is `/usr/bin/notify-send` and the arguments are `Hot` and `Water`
- **THEN** the app starts that program with two arguments and no shell

#### Scenario: Time limit
- **WHEN** the program still runs after 30 s
- **THEN** the app kills it and reports the outcome `timeout`

#### Scenario: Environment
- **WHEN** the Graph `graph_2` named "Hot water" fires with a node that gives 46.2
- **THEN** the program sees the Graph id, the name and the value 46.2 in its environment

#### Scenario: Relative path
- **WHEN** a Graph write has the path `notify-send`
- **THEN** the daemon refuses it with `InvalidArgument`

#### Scenario: Program missing
- **WHEN** the path does not exist in the session
- **THEN** the app reports the outcome `failed`

### Requirement: E-mail action
An e-mail action SHALL have one recipient address, a subject of up to 128 characters and a text of up to 4096 characters. The daemon SHALL send a plain text message through the SMTP account, with a time limit of 30 s, and SHALL strip line breaks from the subject.

#### Scenario: Send
- **WHEN** the action fires and the server accepts the message
- **THEN** the outcome is `ok`

#### Scenario: No account
- **WHEN** no SMTP account is set and the action fires
- **THEN** the outcome is `not-configured`

#### Scenario: Slow server
- **WHEN** the server does not answer in 30 s
- **THEN** the outcome is `timeout` and the tick stays on time

#### Scenario: Subject with a line break
- **WHEN** the subject expands to a text with a line break
- **THEN** the message has the subject on one line

### Requirement: MQTT message action
An MQTT message action SHALL have a topic below the base topic and a payload template of up to 4096 bytes. The daemon SHALL publish `<base topic>/<topic>` with QoS 1 and no retain flag. The topic SHALL have 1 to 128 characters, no wildcard, no empty level and no leading or trailing slash. An empty template SHALL give a JSON object with the Graph id, the Graph name and the time.

#### Scenario: Publish
- **WHEN** the base topic is `nicecooling`, the topic is `alerts/water` and the broker acknowledges the message
- **THEN** the message goes to `nicecooling/alerts/water` and the outcome is `ok`

#### Scenario: Wildcard in the topic
- **WHEN** a Graph write has the topic `alerts/#`
- **THEN** the daemon refuses it with `InvalidArgument`

#### Scenario: MQTT off
- **WHEN** no broker host is set and the action fires
- **THEN** the outcome is `not-configured`

#### Scenario: Broker down
- **WHEN** the broker does not acknowledge in 30 s
- **THEN** the outcome is `timeout` and the message stays in the queue

### Requirement: Power actions
The poweroff, reboot and suspend kinds SHALL have a countdown of 10 to 3600 s, default 30 s. The daemon SHALL run the action through logind after the countdown, with the inhibitor rule of the Alarm poweroff. The daemon SHALL announce the countdown to the apps, and the apps SHALL show the notification with "Cancel" as for the Alarm poweroff. A power action SHALL need no session.

#### Scenario: Countdown ends
- **WHEN** the countdown of a poweroff action is 30 s and nobody cancels
- **THEN** the daemon calls logind to power off after 30 s and the outcome is `ok`

#### Scenario: No session
- **WHEN** no user is logged in and a suspend action fires
- **THEN** the daemon counts down and suspends

#### Scenario: Countdown below the limit
- **WHEN** a Graph write has a countdown of 5 s
- **THEN** the daemon refuses it with `InvalidArgument`

### Requirement: Cancel of a power action
`CancelPoweroff` SHALL cancel a power action countdown by its run id. A cancel SHALL leave the node in the state "run", SHALL run no power call and SHALL give the outcome `cancelled`. A cancel after the logind call has started SHALL fail with `PoweroffStarted`.

#### Scenario: Cancel in time
- **WHEN** the user cancels at second 12 of a 30 s countdown
- **THEN** no power call happens and the outcome is `cancelled`

#### Scenario: Repeat after cancel
- **WHEN** repeat is on at 600 s and the user cancelled at second 12
- **THEN** the action runs again after the repeat interval if the input is still true

#### Scenario: Cancel too late
- **WHEN** the logind call has started
- **THEN** the cancel fails with `PoweroffStarted`

### Requirement: One power action at a time
The daemon SHALL run one power action countdown at a time. A power run that starts while another countdown runs SHALL get the outcome `busy` and SHALL do nothing. A new run of a node that has a run with no outcome yet SHALL get `busy`.

#### Scenario: Two power actions
- **WHEN** a poweroff countdown runs and a reboot action fires
- **THEN** the reboot run gets the outcome `busy`

#### Scenario: Command still running
- **WHEN** a command run of a node has no outcome and the repeat interval passes again
- **THEN** the new run gets the outcome `busy` and the app starts no second program

### Requirement: Alarm poweroff stays separate
No Graph SHALL change the alarm response table or remove the poweroff of an Alarm. The Alarm poweroff path SHALL work with no Graph.

#### Scenario: Graph deleted
- **WHEN** all Graphs are deleted
- **THEN** the pump-stop Alarm still powers off after its delay

#### Scenario: Graph disabled
- **WHEN** a Graph reads the pump-stop event and is disabled
- **THEN** the alarm response of the event is unchanged

### Requirement: Request and outcome exchange
For notification, sound and command the daemon SHALL emit the signal `ActionRequested` with the Graph id, a run id, the kind and the parameters after the placeholders expand. The app SHALL answer with `ReportActionOutcome(run id, outcome)` with `ok`, `failed` or `timeout`. A run id SHALL stay unique across daemon restarts. Only an app in the active session of a seat SHALL run a request.

#### Scenario: Request and answer
- **WHEN** a notification action fires and the app shows it
- **THEN** the app calls `ReportActionOutcome` with `ok` and the daemon writes the log row

#### Scenario: Second report
- **WHEN** two apps report the same run id
- **THEN** the daemon keeps the first outcome

#### Scenario: Unknown run id
- **WHEN** an app reports a run id that is not waiting
- **THEN** the daemon ignores the report

### Requirement: Outcome without a session
When no outcome arrives, the daemon SHALL record the outcome `no-session`. The daemon SHALL wait 10 s for a notification or a sound, and the time limit plus 10 s for a command. A command SHALL not run later in a session that opens after that.

#### Scenario: No app
- **WHEN** a command action with the limit 30 s fires and no app answers
- **THEN** the daemon writes the outcome `no-session` after 40 s

#### Scenario: Late login
- **WHEN** a user logs in after the outcome `no-session`
- **THEN** the app runs nothing for that run id

#### Scenario: Notification without app
- **WHEN** a notification action fires and no app answers in 10 s
- **THEN** the outcome is `no-session`

### Requirement: Message placeholders
The text of a notification, the subject and text of an e-mail and the payload template of an MQTT message SHALL expand `{graph}`, `{time}`, `{value:<label>}` and `{unit:<label>}`, where the label names a node of the Graph. `{{` SHALL give `{` and `}}` SHALL give `}`. A node without a value SHALL give `--`. A text with an unknown label SHALL keep the placeholder as written. A value SHALL have at most 2 decimals, with no trailing zero.

#### Scenario: Value and unit
- **WHEN** the node labelled `water` gives 46.2 and its unit is `°C`, and the text is "Water {value:water} {unit:water}"
- **THEN** the message is "Water 46.2 °C"

#### Scenario: No value
- **WHEN** the node labelled `water` has no value
- **THEN** `{value:water}` gives `--`

#### Scenario: Unknown label
- **WHEN** the text holds `{value:pump}` and no node has the label `pump`
- **THEN** the message holds `{value:pump}` as written

#### Scenario: Escaped brace
- **WHEN** the text holds `{{graph}}`
- **THEN** the message holds `{graph}`

### Requirement: Countdown notification text
The app SHALL show the countdown of a power action with the kind, the Graph name and the seconds left, and SHALL update it once per second. The notification SHALL have critical urgency.

#### Scenario: Countdown shown
- **WHEN** a poweroff action of "Overheat" starts a 30 s countdown
- **THEN** the app shows "Overheat: power off in 30 s" with the button "Cancel"

### Requirement: Action log
The daemon SHALL write one row for each run: time, Graph id, Graph name, kind and outcome. The outcome SHALL be `ok`, `failed`, `timeout`, `no-session`, `not-configured`, `busy` or `cancelled`. The log SHALL hold the last 500 rows in `action-log.json`. The editor SHALL show the last rows of its Graph.

#### Scenario: Row written
- **WHEN** an e-mail action runs and the server accepts
- **THEN** the log has a row with the kind `email` and the outcome `ok`

#### Scenario: Log limit
- **WHEN** the log holds 500 rows and a run ends
- **THEN** the daemon drops the oldest row and keeps 500

#### Scenario: Editor rows
- **WHEN** a Graph has run 3 times
- **THEN** the editor shows those 3 rows

### Requirement: SMTP account
The daemon SHALL hold one SMTP account: host, port, security (STARTTLS, TLS or none), user name, password and sender address. The ports SHALL default to 587, 465 and 25 for the three securities. The daemon SHALL check the server certificate in the secure modes. With the security none, the daemon SHALL send no user name and no password.

#### Scenario: STARTTLS
- **WHEN** the security is STARTTLS and the server certificate is not valid
- **THEN** the send fails with the outcome `failed`

#### Scenario: None with a password
- **WHEN** the security is none and a password is set
- **THEN** the daemon sends the message with no login

#### Scenario: Default port
- **WHEN** the user picks TLS and leaves the port empty
- **THEN** the port is 465

### Requirement: SMTP password storage
The daemon SHALL store the SMTP password in a file of the state directory that only root reads. The D-Bus API SHALL never return the password. A read SHALL return a flag that says whether a password is set. A write without a password argument SHALL keep the stored password, and an empty password SHALL clear it.

#### Scenario: Read settings
- **WHEN** a client reads the SMTP settings
- **THEN** the reply holds the host, port, security, user name, sender and the flag, and no password

#### Scenario: Change the host only
- **WHEN** the user changes the host and sends no password
- **THEN** the stored password stays

#### Scenario: File mode
- **WHEN** the daemon writes the file
- **THEN** its mode is 0600 and its owner is root

### Requirement: Action editor warnings
The editor SHALL warn and SHALL not refuse. It SHALL warn when a power action is chosen ("This action can power off or restart the PC"), when a command path is not an executable file, and when the e-mail or MQTT account is not set. The action node SHALL have a "Test" button.

#### Scenario: Poweroff chosen
- **WHEN** the user picks the kind poweroff
- **THEN** the editor shows the warning and still allows Apply

#### Scenario: Account missing
- **WHEN** the user picks the kind e-mail and no SMTP account is set
- **THEN** the editor shows "No SMTP account" and still allows Apply

### Requirement: SMTP settings screen
The app SHALL show the SMTP settings on the Settings screen, in the group "Network". The password field SHALL show a placeholder when a password is set and SHALL not show the value.

#### Scenario: Password set
- **WHEN** a password is set
- **THEN** the field shows a placeholder of dots and never the stored text

#### Scenario: Daemon offline
- **WHEN** the daemon is unreachable
- **THEN** the screen shows the offline banner and turns the write controls off
