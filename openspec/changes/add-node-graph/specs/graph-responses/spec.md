# Graph responses

## Purpose

Defines the response end node of a Graph: its timing state machine, its stored run state, its eight kinds with their parameters, the response pause, the rules that keep a response from a power loop, the message placeholders, the response log and the SMTP account. The app runs every response in the user session. The daemon runs none.

## ADDED Requirements

### Requirement: Response end node settings
A response end node SHALL have a kind, a hold time, "repeat every" and a minimum interval. The hold time SHALL be 0 to 3600 s, default 0 s. "Repeat every" SHALL be off or 1 to 86400 s, default 60 s when on. The minimum interval SHALL be 0 to 86400 s. Its default SHALL be 600 s for poweroff, reboot and suspend, and 0 s for the other kinds. The node SHALL have no output pin.

#### Scenario: Defaults of a notification
- **WHEN** the user adds a response end node of the kind notification
- **THEN** the hold time is 0 s, repeat is off and the minimum interval is 0 s

#### Scenario: Defaults of a poweroff
- **WHEN** the user picks the kind poweroff
- **THEN** the minimum interval is 600 s

#### Scenario: Hold time above the limit
- **WHEN** a Graph has a hold time of 3601 s
- **THEN** the validator refuses it

### Requirement: Response state machine
The app SHALL evaluate each response end node once per second. An input that is false or has no value SHALL clear the trigger time and set the state "not run". A rise from false to true SHALL set the trigger time to now. In the state "not run", when the trigger time plus the hold time is not after now, the node SHALL run its response and set the state "run".

#### Scenario: Hold time
- **WHEN** the hold time is 10 s and the input rises at second 100 and stays true
- **THEN** the response runs once at second 110

#### Scenario: Input falls inside the hold time
- **WHEN** the hold time is 10 s and the input is true from second 100 to second 105 and then false
- **THEN** the response does not run

#### Scenario: No run on the falling edge
- **WHEN** the input falls after a run
- **THEN** the response does not run and the state returns to "not run"

#### Scenario: No value counts as false
- **WHEN** the input has no value for one second inside the hold time
- **THEN** the trigger time is forgotten and a later true input starts the hold time again

### Requirement: Repeat
In the state "run" with repeat on, the node SHALL run its response again when the last run plus the repeat interval is not after now and the input is still true. Without repeat the node SHALL run once for each rise.

#### Scenario: Repeat
- **WHEN** repeat is on at 60 s and the response ran at second 110 while the input stays true
- **THEN** the response runs again at seconds 170 and 230

#### Scenario: No repeat
- **WHEN** repeat is off and the input stays true for 1 hour after the run
- **THEN** the response runs once

### Requirement: Minimum interval
A run that would start before the last run plus the minimum interval SHALL be dropped. The minimum interval SHALL count wall clock time. A dropped run SHALL leave the state at "run" and SHALL not write a log row. A dropped first run SHALL not be tried again unless repeat is on.

#### Scenario: Second rise inside the interval
- **WHEN** the minimum interval is 300 s, the response ran at second 100, and the input falls and rises at second 200
- **THEN** the run at second 200 is dropped

#### Scenario: Interval over
- **WHEN** the input rises again at second 420
- **THEN** the response runs

#### Scenario: Interval over a reboot
- **WHEN** a poweroff response ran at 10:00:00 with the minimum interval 600 s and the PC starts again at 10:03:00 with the input true
- **THEN** the response does not run before 10:10:00

### Requirement: Stored run state
The app SHALL store the state and the last run time of each response end node with the wall clock. After a restart of the app, a response whose stored state is "run" and whose input is true SHALL stay in the state "run" and SHALL not run again.

#### Scenario: Restart with the condition true
- **WHEN** a response ran, the app restarts, and the input is still true
- **THEN** the state is "run" and the response does not run again

#### Scenario: Restart with the condition gone
- **WHEN** a response ran, the app restarts, and the input is false
- **THEN** the state is "not run"

### Requirement: Start delay
For 10 s after its start the app SHALL evaluate the Graphs and SHALL run no response. An input that is true at the end of the delay, with a stored state other than "run", SHALL count as a rising edge at that time.

#### Scenario: Condition true at login
- **WHEN** the input of a response with hold time 0 s and stored state "not run" is true from the start
- **THEN** the response runs at second 10 after the start and not before

#### Scenario: Graph sensor at login
- **WHEN** a sensor Graph has a value in the first second
- **THEN** the app feeds it in the first second

### Requirement: Run once
"Run once" on a response end node SHALL run the response one time at once with the parameters of the draft and SHALL ignore the minimum interval and the state. It SHALL run in the response pause too, because the user asks for this one run.

#### Scenario: Run once inside the interval
- **WHEN** the response ran 10 s ago with a minimum interval of 300 s and the user presses "Run once"
- **THEN** the response runs again

#### Scenario: Draft parameters
- **WHEN** the user presses "Run once" on a node that is not applied yet
- **THEN** the response runs with the parameters in the editor

#### Scenario: In the pause
- **WHEN** the response pause is on and the user presses "Run once" on a notification response
- **THEN** the notification shows once, and the response pause stays on

### Requirement: Response process
The app SHALL run every response kind in the user session: notification, sound, command, e-mail, MQTT message, poweroff, reboot and suspend. The daemon SHALL run none and SHALL accept no request for a response. Only the app of the active session of logind SHALL run responses. No kind SHALL ask for a password.

#### Scenario: Command runs in the session
- **WHEN** a command response fires
- **THEN** the app starts the program as a child process of the user

#### Scenario: Second session
- **WHEN** a second app runs in a session that is not active
- **THEN** that app runs no response

#### Scenario: App not running
- **WHEN** the app does not run and a condition holds
- **THEN** no response runs

### Requirement: Notification response
A notification response SHALL have a title of 1 to 64 characters, a text of up to 1024 characters and an urgency of low, normal or critical, default normal. The app SHALL show it as a desktop notification.

#### Scenario: Notification
- **WHEN** the response fires with the title "Water hot" and the urgency normal
- **THEN** the app shows a notification with that title

#### Scenario: Empty title
- **WHEN** a Graph has an empty title
- **THEN** the validator refuses it

### Requirement: Sound response
A sound response SHALL have one name from the sound theme of the desktop. The app SHALL play it as the sound hint of a notification that carries the Graph name as title.

#### Scenario: Sound
- **WHEN** the response fires with the sound `dialog-warning`
- **THEN** the app sends a notification with the sound name `dialog-warning`

#### Scenario: Empty sound name
- **WHEN** a Graph has an empty sound name
- **THEN** the validator refuses it

### Requirement: Command response
A command response SHALL have an absolute program path, a list of arguments and a time limit of 1 to 3600 s, default 30 s. The app SHALL start the program with no shell, SHALL discard its output and SHALL kill it, with its process group, at the time limit. The app SHALL set environment variables with the Graph id, the Graph name, the run id and the value of each node.

#### Scenario: Run with arguments
- **WHEN** the path is `/usr/bin/notify-send` and the arguments are `Hot` and `Water`
- **THEN** the app starts that program with two arguments and no shell

#### Scenario: Time limit
- **WHEN** the program still runs after 30 s
- **THEN** the app kills it and the outcome is `timeout`

#### Scenario: Environment
- **WHEN** the Graph `graph_2` named "Hot water" fires with a node that gives 46.2
- **THEN** the program sees the Graph id, the name and the value 46.2 in its environment

#### Scenario: Relative path
- **WHEN** a Graph has the path `notify-send`
- **THEN** the validator refuses it

#### Scenario: Program missing
- **WHEN** the path does not exist
- **THEN** the outcome is `failed`

### Requirement: E-mail response
An e-mail response SHALL have one recipient address, a subject of up to 128 characters and a text of up to 4096 characters. The app SHALL send a plain text message through the SMTP account, with a time limit of 30 s, outside the evaluation, and SHALL strip line breaks from the subject.

#### Scenario: Send
- **WHEN** the response fires and the server accepts the message
- **THEN** the outcome is `ok`

#### Scenario: No account
- **WHEN** no SMTP account is set and the response fires
- **THEN** the outcome is `not-configured`

#### Scenario: Slow server
- **WHEN** the server does not answer in 30 s
- **THEN** the outcome is `timeout` and the evaluation stays on time

#### Scenario: Subject with a line break
- **WHEN** the subject expands to a text with a line break
- **THEN** the message has the subject on one line

### Requirement: MQTT message response
An MQTT message response SHALL have a topic below the base topic and a payload template of up to 4096 bytes. The app SHALL publish `<base topic>/<topic>` with QoS 1 and no retain flag. The topic SHALL have 1 to 128 characters, no wildcard, no empty level and no leading or trailing slash. An empty template SHALL give a JSON object with the Graph id, the Graph name and the time.

#### Scenario: Publish
- **WHEN** the base topic is `nicecooling`, the topic is `alerts/water` and the broker acknowledges the message
- **THEN** the message goes to `nicecooling/alerts/water` and the outcome is `ok`

#### Scenario: Wildcard in the topic
- **WHEN** a Graph has the topic `alerts/#`
- **THEN** the validator refuses it

#### Scenario: MQTT off
- **WHEN** no broker host is set and the response fires
- **THEN** the outcome is `not-configured`

#### Scenario: Broker down
- **WHEN** the broker does not acknowledge in 30 s
- **THEN** the outcome is `timeout` and the message stays in the queue

### Requirement: Power responses
The poweroff, reboot and suspend kinds SHALL have a countdown of 10 to 3600 s, default 30 s. After the countdown the app SHALL call logind with the rights of the session, with no interactive authorization. When logind refuses, the outcome SHALL be `failed` and the app SHALL not ask for a password. The app SHALL show the countdown with a button "Cancel" in a notification of critical urgency, in the tray menu and in the window.

#### Scenario: Countdown ends
- **WHEN** the countdown of a poweroff response is 30 s and nobody cancels
- **THEN** the app calls logind to power off after 30 s and the outcome is `ok`

#### Scenario: Logind refuses
- **WHEN** another user is logged in and logind refuses the poweroff
- **THEN** the outcome is `failed` and no password prompt appears

#### Scenario: Countdown below the limit
- **WHEN** a Graph has a countdown of 5 s
- **THEN** the validator refuses it

#### Scenario: App stops in the countdown
- **WHEN** the app stops 10 s into a 30 s countdown
- **THEN** no power call happens

### Requirement: Cancel of a power response
A cancel SHALL stop the countdown, SHALL run no power call and SHALL give the outcome `cancelled`. It SHALL leave the node in the state "run". A cancel after the logind call SHALL have no effect.

#### Scenario: Cancel in time
- **WHEN** the user cancels at second 12 of a 30 s countdown
- **THEN** no power call happens and the outcome is `cancelled`

#### Scenario: Repeat after cancel
- **WHEN** repeat is on at 600 s and the user cancelled at second 12
- **THEN** the response can run again after the repeat interval while the input is true

#### Scenario: Countdown text
- **WHEN** a poweroff response of "Overheat" starts a 30 s countdown
- **THEN** the app shows "Overheat: power off in 30 s" with the button "Cancel" and updates it once per second

### Requirement: One run at a time
The app SHALL run one power countdown at a time. A power run that starts while another countdown runs SHALL get the outcome `busy`. A new run of a node that has a run with no outcome yet SHALL get `busy`.

#### Scenario: Two power responses
- **WHEN** a poweroff countdown runs and a reboot response fires
- **THEN** the reboot run gets the outcome `busy`

#### Scenario: Command still running
- **WHEN** a command run has no outcome and the repeat interval passes again
- **THEN** the new run gets the outcome `busy` and the app starts no second program

### Requirement: Alarm poweroff wins
While the daemon reports a poweroff countdown of an Alarm, the app SHALL start no power response and SHALL cancel its own power countdown. No Graph SHALL change the alarm response table or delay the poweroff of an Alarm.

#### Scenario: Alarm countdown starts
- **WHEN** a Graph poweroff countdown has 20 s left and the pump-stop Alarm starts its poweroff countdown
- **THEN** the app cancels the Graph countdown with the outcome `cancelled`

#### Scenario: Graph power response during an Alarm countdown
- **WHEN** a power response fires while an Alarm poweroff countdown runs
- **THEN** it does not start

#### Scenario: No Graph
- **WHEN** all Graphs are deleted
- **THEN** the pump-stop Alarm still powers off after its delay

### Requirement: Response pause
The app SHALL count the poweroff and reboot runs that it starts. The count SHALL go back to 0 when the app has run 10 minutes since the last such run. At a count of 3 the app SHALL enter the response pause: no Graph response runs and the Graph sensors still work. The pause SHALL stay over a restart. The app SHALL show a banner with "Turn the Graph responses on", which sets the count to 0 and ends the pause.

#### Scenario: Third reboot
- **WHEN** reboot responses run 3 times with less than 10 minutes of app time between them
- **THEN** the app enters the response pause at the third run

#### Scenario: Count resets
- **WHEN** one poweroff ran and the app then runs 10 minutes
- **THEN** the count is 0

#### Scenario: Pause over a restart
- **WHEN** the app restarts in the response pause
- **THEN** the pause is still on and the banner shows

#### Scenario: Turn on
- **WHEN** the user presses "Turn the Graph responses on"
- **THEN** the pause ends and the count is 0

#### Scenario: Cancelled run
- **WHEN** the user cancels a poweroff countdown
- **THEN** the count does not change

### Requirement: Message placeholders
The text of a notification, the subject and text of an e-mail and the payload template of an MQTT message SHALL expand `{graph}`, `{time}`, `{value:<label>}` and `{unit:<label>}`, where the label names a node of the Graph. `{time}` SHALL be the local date and time as `YYYY-MM-DD HH:MM:SS`. `{{` SHALL give `{` and `}}` SHALL give `}`. A node without a value SHALL give `--`. An unknown label SHALL keep the placeholder as written. A value SHALL have at most 2 decimals, with no trailing zero.

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

### Requirement: Response log
The app SHALL write one row for each run: time, Graph id, Graph name, kind and outcome, in `response-log.json` in `$XDG_DATA_HOME/nicecooling/`. The outcome SHALL be `ok`, `failed`, `timeout`, `not-configured`, `busy` or `cancelled`. The log SHALL hold the newest 500 rows. The editor SHALL show the newest rows of its Graph.

#### Scenario: Row written
- **WHEN** an e-mail response runs and the server accepts
- **THEN** the log has a row with the kind `email` and the outcome `ok`

#### Scenario: Log limit
- **WHEN** the log holds 500 rows and a run ends
- **THEN** the app drops the oldest row and keeps 500

#### Scenario: Editor rows
- **WHEN** a Graph has run 3 times
- **THEN** the editor shows those 3 rows

### Requirement: SMTP account
The app SHALL hold one SMTP account: host, port, security (STARTTLS, TLS or none), user name, password and sender address. The ports SHALL default to 587, 465 and 25 for the three securities. The app SHALL check the server certificate in the secure modes. With the security none and a user name set, the app SHALL log in and the settings SHALL show a warning that the login is not encrypted.

#### Scenario: STARTTLS
- **WHEN** the security is STARTTLS and the server certificate is not valid
- **THEN** the send fails with the outcome `failed`

#### Scenario: None with a password
- **WHEN** the security is none and a user name and password are set
- **THEN** the app logs in and sends the message, and the settings show the warning

#### Scenario: Default port
- **WHEN** the user picks TLS and leaves the port empty
- **THEN** the port is 465

### Requirement: SMTP password storage
The app SHALL store the SMTP password in `secrets.toml` in `$XDG_CONFIG_HOME/nicecooling/` with the mode 0600. The app SHALL never show the stored password. The password field SHALL show a placeholder when a password is set. The app SHALL never write the password to a log or to an export file.

#### Scenario: File mode
- **WHEN** the app writes `secrets.toml`
- **THEN** its mode is 0600

#### Scenario: Change the host only
- **WHEN** the user changes the host and leaves the password field untouched
- **THEN** the stored password stays

#### Scenario: Placeholder
- **WHEN** a password is set
- **THEN** the field shows a placeholder of dots and never the stored text

### Requirement: Response editor warnings
The editor SHALL warn and SHALL not refuse a valid choice. It SHALL warn when a power kind is chosen ("This response can power off or restart the PC"), when a command path is not an executable file, and when the e-mail or MQTT account is not set. The response end node SHALL have a "Run once" button.

#### Scenario: Poweroff chosen
- **WHEN** the user picks the kind poweroff
- **THEN** the editor shows the warning and still allows Apply

#### Scenario: Account missing
- **WHEN** the user picks the kind e-mail and no SMTP account is set
- **THEN** the editor shows "No SMTP account" and still allows Apply

### Requirement: Response pause banner
The app SHALL show the banner of the response pause on every screen until the user turns the responses on. The banner SHALL name the cause: "3 power responses in a row".

#### Scenario: Banner shown
- **WHEN** the response pause is on and the user opens the Sensors screen
- **THEN** the banner is visible with the button "Turn the Graph responses on"

### Requirement: SMTP settings screen
The app SHALL show the SMTP settings on the Settings screen, in the group "Network".

#### Scenario: Settings card
- **WHEN** the user opens the Settings screen
- **THEN** the group "Network" shows the SMTP fields and the unencrypted-login warning when it applies
