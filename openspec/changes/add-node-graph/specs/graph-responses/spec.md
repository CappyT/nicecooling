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
The app SHALL evaluate each response end node once per second. A false input SHALL clear the trigger time and set the state "not run". An input with no value SHALL keep the state and clear the trigger time. An input that is true after false or no value SHALL set the trigger time to now. In the state "not run", when the trigger time plus the hold time is not after now, the node SHALL run its response and set the state "run".

#### Scenario: Hold time
- **WHEN** the hold time is 10 s and the input rises at second 100 and stays true
- **THEN** the response runs once at second 110

#### Scenario: Input falls inside the hold time
- **WHEN** the hold time is 10 s and the input is true from second 100 to second 105 and then false
- **THEN** the response does not run

#### Scenario: No run on the falling edge
- **WHEN** the input falls after a run
- **THEN** the response does not run and the state returns to "not run"

#### Scenario: No value in the state run
- **WHEN** the node is in the state "run", the input has no value for 30 s and then reads 1 again
- **THEN** the state stays "run" and the response does not run again

#### Scenario: No value in the hold time
- **WHEN** the hold time is 10 s, the input is true from second 100, has no value at second 105 and is true again from second 106
- **THEN** the response runs at second 116

### Requirement: Repeat
In the state "run" with repeat on, the node SHALL run its response again when the last run plus the repeat interval is not after now and the input is still true. After a start of the app with the stored state "run", the repeat interval SHALL count from the start of the app. Without repeat the node SHALL run once for each rise.

#### Scenario: Repeat
- **WHEN** repeat is on at 60 s and the response ran at second 110 while the input stays true
- **THEN** the response runs again at seconds 170 and 230

#### Scenario: Repeat after a restart
- **WHEN** repeat is on at 60 s, the state is stored as "run" and the app starts at second 500 with the input true
- **THEN** the response runs at second 560

#### Scenario: Repeat after a dropped run
- **WHEN** repeat is on at 60 s, the minimum interval is 600 s, and the repeat at second 170 is dropped
- **THEN** the next repeat is due at second 230: a dropped run counts as an attempt for the repeat interval

#### Scenario: No repeat
- **WHEN** repeat is off and the input stays true for 1 hour after the run
- **THEN** the response runs once

### Requirement: Minimum interval
A run that would start before the last run plus the minimum interval SHALL be dropped. The minimum interval SHALL count wall clock time. A dropped run SHALL leave the state at "run" and SHALL not write a log row. A dropped first run SHALL not be tried again unless repeat is on. A last run that is later than the wall clock SHALL be set to the wall clock, so the wait is at most one minimum interval.

#### Scenario: Second rise inside the interval
- **WHEN** the minimum interval is 300 s, the response ran at second 100, and the input falls and rises at second 200
- **THEN** the run at second 200 is dropped

#### Scenario: Interval over
- **WHEN** the input rises again at second 420
- **THEN** the response runs

#### Scenario: Wall clock goes back
- **WHEN** a poweroff response ran at 10:00:00 with the minimum interval 600 s, and the wall clock is set back to 09:00:00 with the input true
- **THEN** the response does not run before 09:10:00 and runs after

#### Scenario: Wall clock goes back with no interval
- **WHEN** a notification response with the minimum interval 0 s ran at 10:00:00, and the wall clock is set back to 09:00:00 with the input true
- **THEN** the response runs at once

#### Scenario: Interval over a reboot
- **WHEN** a poweroff response ran at 10:00:00 with the minimum interval 600 s and the PC starts again at 10:03:00 with the input true
- **THEN** the response does not run before 10:10:00

### Requirement: Stored run state
The app SHALL store the state and the last run time of each response end node with the wall clock, in `graph-state.json`. After a restart of the app, a node whose stored state is "run" and whose input is true SHALL stay in the state "run" and SHALL not run again. A node whose stored state is "not run" and whose input is true SHALL see a rise at the first evaluation.

#### Scenario: Restart with the condition true
- **WHEN** a response ran, the app restarts, and the input is still true
- **THEN** the state is "run" and the response does not run again

#### Scenario: Restart with the condition gone
- **WHEN** a response ran, the app restarts, and the input is false
- **THEN** the state is "not run"

#### Scenario: Not run before the restart
- **WHEN** a response did not run, the app restarts, and the input is true at the first evaluation with hold time 0 s
- **THEN** the response runs at the first evaluation

### Requirement: Run once
"Run once" on a response end node SHALL run the response one time at once with the parameters of the draft. It SHALL ignore the state, the minimum interval and the Response pause. A poweroff or reboot from "Run once" SHALL count for the Response pause. It SHALL not run a power response while the daemon reports a poweroff countdown of an Alarm.

#### Scenario: Run once inside the interval
- **WHEN** the response ran 10 s ago with a minimum interval of 300 s and the user presses "Run once"
- **THEN** the response runs again

#### Scenario: Draft parameters
- **WHEN** the user presses "Run once" on a node that is not applied yet
- **THEN** the response runs with the parameters in the editor

#### Scenario: In the pause
- **WHEN** the Response pause is on and the user presses "Run once" on a notification response
- **THEN** the response runs

#### Scenario: Counts for the pause
- **WHEN** the count is 2 and the user presses "Run once" on a reboot response
- **THEN** the count is 3 and the pause starts

#### Scenario: Alarm countdown
- **WHEN** an Alarm poweroff countdown runs and the user presses "Run once" on a poweroff response
- **THEN** nothing runs and the app shows the reason

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

### Requirement: Active session
Only the app of the active session SHALL run responses. The app SHALL find its session with the id in `XDG_SESSION_ID`, call `GetSession` on `org.freedesktop.login1.Manager` at `/org/freedesktop/login1` on the system bus, and read the property `Active` of `org.freedesktop.login1.Session`. It SHALL follow the signal `PropertiesChanged` of that property.

#### Scenario: Active
- **WHEN** `Active` is true
- **THEN** the app runs responses

#### Scenario: Inactive
- **WHEN** `Active` is false
- **THEN** the app runs no response and keeps evaluating the Graphs

#### Scenario: Session switch
- **WHEN** the user switches to another session and back
- **THEN** the app runs no response while `Active` is false and runs again when it is true

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
A command response SHALL have an absolute program path, a list of arguments and a time limit of 1 to 3600 s, default 30 s. The app SHALL start the program with no shell, SHALL discard its output and SHALL kill it, with its process group, at the time limit. The app SHALL set `NICECOOLING_GRAPH_ID`, `NICECOOLING_GRAPH_NAME`, `NICECOOLING_RUN_ID` and `NICECOOLING_NODE_<LABEL>` for each labelled node, with the label in upper case. The value SHALL be decimal text, or empty with no value.

#### Scenario: Run with arguments
- **WHEN** the path is `/usr/bin/notify-send` and the arguments are `Hot` and `Water`
- **THEN** the app starts that program with two arguments and no shell

#### Scenario: Time limit
- **WHEN** the program still runs after 30 s
- **THEN** the app kills it and the outcome is `timeout`

#### Scenario: Environment
- **WHEN** the Graph `graph_2` named "Hot water" fires, the node labelled `water` gives 46.2, the node labelled `pump` has no value and a third node has no label
- **THEN** the program sees `NICECOOLING_GRAPH_ID=graph_2`, `NICECOOLING_GRAPH_NAME=Hot water`, `NICECOOLING_NODE_WATER=46.2` and `NICECOOLING_NODE_PUMP=` empty, and no variable for the third node

#### Scenario: Environment of the app
- **WHEN** the app runs with `HOME=/home/anna` and a command response starts
- **THEN** the program sees `HOME=/home/anna` and the `NICECOOLING_` variables of the run

#### Scenario: Relative path
- **WHEN** a Graph has the path `notify-send`
- **THEN** the validator refuses it with the code `bad-parameter`

#### Scenario: Program missing
- **WHEN** the path does not exist
- **THEN** the outcome is `failed`

### Requirement: Command environment values
The value of `NICECOOLING_NODE_<LABEL>` SHALL be the node value rounded to 8 decimals, with a tie away from 0, as decimal text with no trailing zero and no exponent.

#### Scenario: Sum of two decimals
- **WHEN** the node labelled `sum` gives 0.30000000000000004
- **THEN** `NICECOOLING_NODE_SUM` is `0.3`

#### Scenario: Eight decimals
- **WHEN** the node labelled `water` gives 46.1234567826
- **THEN** `NICECOOLING_NODE_WATER` is `46.12345678`

#### Scenario: Whole number
- **WHEN** the node labelled `rpm` gives 3000.000000001
- **THEN** `NICECOOLING_NODE_RPM` is `3000`

### Requirement: E-mail response
An e-mail response SHALL have one recipient address, a subject of up to 128 characters and a text of up to 4096 characters. The app SHALL send a plain text message through the SMTP account, with a time limit of 30 s, outside the evaluation, and SHALL replace each run of CR and LF in the subject with one space.

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
- **WHEN** the subject expands to `Hot\nBcc: x@example.org`
- **THEN** the message has the one subject line `Hot Bcc: x@example.org` and no `Bcc` header

#### Scenario: Subject with a run of line breaks
- **WHEN** the subject expands to `Hot\r\n\r\nwater`
- **THEN** the subject line is `Hot water`

### Requirement: MQTT message response
An MQTT message response SHALL have a topic below the base topic and a payload template of up to 4096 bytes. The app SHALL publish `<base topic>/<topic>` with QoS 1 and no retain flag. The topic SHALL have 1 to 128 characters, no wildcard, no empty level and no leading or trailing slash. An empty template SHALL give the JSON object `{"graph_id":"<id>","graph_name":"<name>","time":<microseconds since the Unix epoch>}`.

#### Scenario: Publish
- **WHEN** the base topic is `nicecooling`, the topic is `alerts/water` and the broker acknowledges the message
- **THEN** the message goes to `nicecooling/alerts/water` and the outcome is `ok`

#### Scenario: Default payload
- **WHEN** the template is empty and `graph_2` named "Hot water" fires at 1766000000000000 us
- **THEN** the payload is `{"graph_id":"graph_2","graph_name":"Hot water","time":1766000000000000}`

#### Scenario: Wildcard in the topic
- **WHEN** a Graph has the topic `alerts/#`
- **THEN** the validator refuses it with the code `bad-parameter`

#### Scenario: MQTT off
- **WHEN** no broker host is set and the response fires
- **THEN** the outcome is `not-configured`

#### Scenario: Broker down
- **WHEN** the broker does not acknowledge in 30 s
- **THEN** the outcome is `timeout` and the message stays in the queue

### Requirement: Power responses
The poweroff, reboot and suspend kinds SHALL have a countdown of 10 to 3600 s, default 30 s. After the countdown the app SHALL call `PowerOff`, `Reboot` or `Suspend` on `org.freedesktop.login1.Manager` with the argument `interactive` false. When logind refuses, the outcome SHALL be `failed` and the app SHALL not ask for a password. The app SHALL show the countdown with "Cancel" in a notification of critical urgency, in the tray menu and in the window.

#### Scenario: Countdown ends
- **WHEN** the countdown of a poweroff response is 30 s and nobody cancels
- **THEN** the app calls `PowerOff(false)` after 30 s and the outcome is `ok`

#### Scenario: Logind refuses
- **WHEN** another user is logged in and logind refuses the poweroff
- **THEN** the outcome is `failed` and no password prompt appears

#### Scenario: Countdown below the limit
- **WHEN** a Graph has a countdown of 5 s
- **THEN** the validator refuses it with the code `bad-parameter`

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
While the daemon reports a poweroff countdown of an Alarm, the app SHALL start no power response, "Run once" included, and SHALL cancel its own power countdown. No Graph SHALL change the alarm response table or delay the poweroff of an Alarm.

#### Scenario: Alarm countdown starts
- **WHEN** a Graph poweroff countdown has 20 s left and the pump-stop Alarm starts its poweroff countdown
- **THEN** the app cancels the Graph countdown with the outcome `cancelled`

#### Scenario: Graph power response during an Alarm countdown
- **WHEN** a power response fires while an Alarm poweroff countdown runs
- **THEN** it does not start

#### Scenario: No Graph
- **WHEN** all Graphs are deleted
- **THEN** the pump-stop Alarm still powers off after its delay

### Requirement: Stop of the app
"Quit" and the end of the session (SIGTERM) SHALL take one path. The app SHALL tell the Graph service to stop. The service SHALL write `graph-state.json` and cancel a running countdown with no power call. The stop message SHALL come before every event, command and timer, so a countdown that ended while "Quit" waited makes no power call. The app SHALL wait for it for at most 2 s before it quits.

#### Scenario: Quit in a countdown
- **WHEN** a poweroff countdown has 20 s left and the user chooses "Quit"
- **THEN** the app writes `graph-state.json`, makes no power call and quits within 2 s

#### Scenario: Countdown ends while Quit waits
- **WHEN** a pass blocks for 3 s in a call to the daemon, a poweroff countdown ends 1 s after "Quit", and the stop message and a daemon event wait with it
- **THEN** the service reads the stop message first and makes no power call

#### Scenario: SIGTERM
- **WHEN** the session ends and the app gets SIGTERM
- **THEN** the app takes the same path

#### Scenario: Service does not answer
- **WHEN** the service does not stop in 2 s
- **THEN** the app quits after 2 s

#### Scenario: Service has ended
- **WHEN** the service thread ended after a panic and the user chooses "Quit"
- **THEN** the app does not claim that the state is written, logs it and quits at once

### Requirement: Response pause
The app SHALL count the poweroff and reboot runs that it starts, "Run once" included. The count SHALL go back to 0 after 60 minutes of app run time with no such run. The app SHALL store this run time, so it adds up over restarts. At a count of 3 the app SHALL enter the Response pause: no Graph response runs by itself and the Graph sensors still work. The pause SHALL stay over a restart. A banner with "Turn the Graph responses on" SHALL set the count to 0 and end the pause.

#### Scenario: Third reboot
- **WHEN** reboot responses start 3 times with less than 60 minutes of app run time between them
- **THEN** the app enters the Response pause at the third run

#### Scenario: Count resets
- **WHEN** one poweroff ran and the app then runs 60 minutes
- **THEN** the count is 0

#### Scenario: Run time over a restart
- **WHEN** the app runs 40 minutes, restarts, and runs 20 minutes
- **THEN** the count is 0

#### Scenario: Pause over a restart
- **WHEN** the app restarts in the Response pause
- **THEN** the pause is still on and the banner shows

#### Scenario: Turn on
- **WHEN** the user presses "Turn the Graph responses on"
- **THEN** the pause ends and the count is 0

#### Scenario: Cancelled run
- **WHEN** the user cancels a poweroff countdown
- **THEN** the count and the quiet time are the values from before that run started

#### Scenario: Suspend
- **WHEN** a suspend response runs
- **THEN** the count does not change

### Requirement: Cancelled power run
A poweroff or reboot run that the user cancels, or that stops with the app before the power call, SHALL give back the count and the quiet time that its start changed. A pause that this run started SHALL end.

#### Scenario: Cancelled run and the 60 minutes
- **WHEN** one poweroff ran, a second poweroff starts 50 minutes later, the user cancels it, and 10 minutes pass
- **THEN** the count is 0

### Requirement: Message placeholders
The text of a notification, the subject and text of an e-mail and the payload template of an MQTT message SHALL expand `{graph}`, `{time}`, `{value:<label>}` and `{unit:<label>}`, where the label names a node of the Graph. `{time}` SHALL be the local date and time as `YYYY-MM-DD HH:MM:SS`. `{{` SHALL give `{` and `}}` SHALL give `}`.

#### Scenario: Value and unit
- **WHEN** the node labelled `water` gives 46.2 and its unit is `°C`, and the text is "Water {value:water} {unit:water}"
- **THEN** the message is "Water 46.2 °C"

#### Scenario: Escaped brace
- **WHEN** the text holds `{{graph}}`
- **THEN** the message holds `{graph}`

#### Scenario: Time
- **WHEN** the response runs at 14:05:09 on 2026-11-25 and the text is "{time}"
- **THEN** the message is "2026-11-25 14:05:09"

### Requirement: Placeholder values
A node without a value SHALL give `--`. An unknown label SHALL keep the placeholder as written. A value SHALL have at most 2 decimals, a tie rounded away from 0, with no trailing zero. The unit of a sensor source node SHALL be the display text of its sensor unit in the app (`°C` for `degC`). The unit of every other node SHALL be empty.

#### Scenario: No value
- **WHEN** the node labelled `water` has no value
- **THEN** `{value:water}` gives `--`

#### Scenario: Tie
- **WHEN** the node labelled `water` gives 0.125
- **THEN** `{value:water}` gives `0.13`

#### Scenario: Unknown label
- **WHEN** the text holds `{value:pump}` and no node has the label `pump`
- **THEN** the message holds `{value:pump}` as written

#### Scenario: Decimals
- **WHEN** a node gives 46.2000001
- **THEN** `{value:water}` gives `46.2`

#### Scenario: Unit of a node that is not a source
- **WHEN** the node labelled `limit` is a constant with the value 50 and the text is "Limit {value:limit}{unit:limit}"
- **THEN** the message is "Limit 50"

### Requirement: Response log
`response-log.json` in `$XDG_DATA_HOME/nicecooling/` SHALL hold `schema_version` (1) and `rows`. A row SHALL have `time_us`, `graph_id`, `graph_name`, `kind` and `outcome`. The outcome SHALL be `ok`, `failed`, `timeout`, `not-configured`, `busy` or `cancelled`. The app SHALL write one row for each run and keep the newest 500. The editor SHALL show the newest rows of its Graph.

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
The app SHALL hold one SMTP account in `settings.toml`, table `[smtp]`: `host`, `port`, `security` (`starttls`, `tls` or `none`), `user` and `sender`. The password SHALL be in `secrets.toml`. The ports SHALL default to 587, 465 and 25 for the three securities. The app SHALL check the server certificate in the secure modes. With the security `none` and a user set, the app SHALL log in and the settings SHALL show a warning that the login is not encrypted.

#### Scenario: STARTTLS
- **WHEN** the security is `starttls` and the server certificate is not valid
- **THEN** the send fails with the outcome `failed`

#### Scenario: None with a password
- **WHEN** the security is `none` and a user and password are set
- **THEN** the app logs in and sends the message, and the settings show the warning

#### Scenario: Default port
- **WHEN** the user picks `tls` and leaves the port empty
- **THEN** the port is 465

### Requirement: SMTP password storage
`secrets.toml` in `$XDG_CONFIG_HOME/nicecooling/` SHALL have the mode 0600 and the keys `schema_version` (1), `[mqtt] password` and `[smtp] password`. The app SHALL never show the stored password. The password field SHALL show a placeholder when a password is set. The app SHALL never write a password to a log or to an export file.

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
The app SHALL show the SMTP settings on the Settings screen, in the group "Network". The SMTP fields SHALL work while the daemon is offline, because the app holds these settings and the daemon has no part in them.

#### Scenario: Settings card
- **WHEN** the user opens the Settings screen
- **THEN** the group "Network" shows the SMTP fields and the unencrypted-login warning when it applies

#### Scenario: Daemon offline
- **WHEN** the daemon is offline and the user sets the host `mail.lan` and the security `tls`
- **THEN** the app stores both in `settings.toml`, and the port is 465

