# MQTT publishing

## Purpose

Defines the MQTT client of the app: the broker settings, the connection and reconnect rules, the pending values and the queue, the topics with their payload, QoS and retained flag, and the storage of the password. The client only publishes. It never subscribes. The daemon has no MQTT code.

## ADDED Requirements

### Requirement: Broker settings
The app SHALL hold one broker in `settings.toml`, table `[mqtt]`: `host`, `port`, `tls`, `ca_file`, `user`, `client_id`, `base_topic` and `interval_s`. The password SHALL be in `secrets.toml`. The port SHALL default to 1883, or 8883 with TLS. The client id SHALL default to `nicecooling-<hostname>`. The base topic SHALL default to `nicecooling`. The interval SHALL be 1 s to 1 h, default 10 s. The settings SHALL belong to the user.

#### Scenario: Defaults
- **WHEN** the user sets only the host `broker.lan` and TLS on
- **THEN** the port is 8883, the client id is `nicecooling-<hostname>`, the base topic is `nicecooling` and the interval is 10 s

#### Scenario: Interval above the limit
- **WHEN** the user sets the interval 7200 s
- **THEN** the app refuses it

#### Scenario: Wildcard in the base topic
- **WHEN** the user sets the base topic `home/#`
- **THEN** the app refuses it

### Requirement: MQTT off until configured
MQTT SHALL be off while the host is empty. In that state the app SHALL open no connection and SHALL queue nothing. A change of any setting SHALL close the connection and open a new one with the new settings.

#### Scenario: Fresh install
- **WHEN** no host is set
- **THEN** the app makes no network connection for MQTT

#### Scenario: Host cleared
- **WHEN** the user clears the host while connected
- **THEN** the app closes the connection and drops the queue

### Requirement: Connection
The client SHALL use MQTT 3.1.1 with a clean session and a keep-alive of 30 s. With TLS on it SHALL check the server certificate against the system certificates and the optional CA file. It SHALL offer no way to skip the check. It SHALL send the user name and password only when a user name is set.

#### Scenario: Invalid certificate
- **WHEN** TLS is on and the broker certificate is not valid
- **THEN** the connection fails and the client retries by the reconnect rule

#### Scenario: No user name
- **WHEN** the user name is empty
- **THEN** the client sends no credentials

### Requirement: Reconnect
After a lost or failed connection the client SHALL wait 1 s, then double the wait after each failure up to 60 s, and SHALL reset the wait to 1 s after a connection that lasted 30 s or more.

#### Scenario: Waits
- **WHEN** the broker is down for 5 attempts
- **THEN** the waits are 1, 2, 4, 8 and 16 s

#### Scenario: Wait limit
- **WHEN** the broker is down for 10 attempts
- **THEN** no wait is longer than 60 s

#### Scenario: Reset
- **WHEN** a connection lasted 40 s and then ends
- **THEN** the next wait is 1 s

### Requirement: No block of the evaluation
The MQTT client SHALL run in its own task. A broker that is unreachable or slow SHALL never delay the evaluation of the Graphs, the sending of the Graph sensor values or the interface of the app.

#### Scenario: Broker unreachable
- **WHEN** the broker host does not answer
- **THEN** the Graphs still evaluate once per second

#### Scenario: Slow publish
- **WHEN** the broker takes 20 s to accept a message
- **THEN** no evaluation runs late

### Requirement: Pending values and queue
The client SHALL keep one pending value for each sensor topic, where a new value replaces the old one. It SHALL keep a queue of at most 256 messages for events and responses. When the queue is full it SHALL drop the oldest message, SHALL write a line to the journal and SHALL give the outcome `failed` to a dropped response message. The queue SHALL stay across reconnects.

#### Scenario: New value replaces
- **WHEN** the broker is down and a sensor changes 100 times
- **THEN** the client holds one pending value for that sensor and sends the newest after the reconnect

#### Scenario: Event waits
- **WHEN** an event message is queued while the broker is down for 2 minutes
- **THEN** the client sends it after the reconnect

#### Scenario: Queue full
- **WHEN** the queue holds 256 messages and another arrives
- **THEN** the oldest message is dropped

### Requirement: Status topic
The client SHALL publish `online` to `<base>/status` with QoS 1 and the retain flag after each connect. It SHALL set the last will `offline` on the same topic with QoS 1 and the retain flag. After the app stops, the broker SHALL publish `offline`, so a reader knows that the retained values are old.

#### Scenario: Connect
- **WHEN** the client connects
- **THEN** `<base>/status` holds `online`, retained

#### Scenario: App stops
- **WHEN** the app stops without a clean disconnect
- **THEN** the broker publishes `offline` on `<base>/status`

### Requirement: Sensor topics
For each sensor that the user has not hidden in the app, the client SHALL publish one JSON object to `<base>/sensor/<sensor id>` at each value interval and at once after a connect, with QoS 0 and the retain flag. The object SHALL have `value`, `unit`, `name` and `time`. `time` SHALL be microseconds since the Unix epoch. The sensors SHALL be those that the app knows: measured, Derived and Graph sensors.

#### Scenario: Value message
- **WHEN** the sensor `nct6798_temp1` named "CPU" reads 46.2 °C
- **THEN** the topic `nicecooling/sensor/nct6798_temp1` gets an object with `value` 46.2, `unit` "°C" and `name` "CPU"

#### Scenario: Hidden sensor
- **WHEN** the user hides a sensor in the app
- **THEN** the client publishes no message for it at the next interval

#### Scenario: Graph sensor
- **WHEN** a Graph sensor `graph_3` exists
- **THEN** the client publishes it like any other sensor

### Requirement: Sensor without a value
A sensor that has no value SHALL publish an object with `value` set to `null`, so the reader sees the loss. A Graph sensor of a disabled Graph SHALL do the same.

#### Scenario: Lost sensor
- **WHEN** a sensor has no value
- **THEN** its next message has `value` null

#### Scenario: Disabled Graph
- **WHEN** the user disables a sensor Graph
- **THEN** its next message has `value` null

### Requirement: Event topics
The client SHALL publish one JSON object to `<base>/event/<event id>` for each raise and each clear of an Alarm or Warning that the daemon reports, with QoS 1 and no retain flag. The object SHALL have `event`, `source`, `severity` (`alarm` or `warning`), `state` (`raised` or `cleared`), `time` (microseconds since the Unix epoch) and `details`. `details` SHALL hold the keys of the event, such as `value`, `limit` and `unit`, when they apply.

#### Scenario: Raise
- **WHEN** the pump-stop Alarm of `loop_1` rises
- **THEN** `nicecooling/event/pump-stop` gets an object with `source` `loop_1`, `severity` `alarm` and `state` `raised`

#### Scenario: Clear
- **WHEN** that Alarm closes
- **THEN** the topic gets an object with `state` `cleared`

#### Scenario: Event id with a dot
- **WHEN** the event `leakshield.leak-alarm` rises
- **THEN** the topic is `nicecooling/event/leakshield.leak-alarm`

### Requirement: Response message topic
The message of an MQTT message response SHALL use its own topic below the base topic, QoS 1 and no retain flag, as the graph-responses spec states. The app SHALL give the response its outcome when the broker acknowledges the message, when the queue drops it, or after 30 s without an answer.

#### Scenario: Acknowledged
- **WHEN** the broker acknowledges the message after 1 s
- **THEN** the outcome of the response is `ok`

#### Scenario: Late acknowledgment
- **WHEN** the broker acknowledges after 40 s
- **THEN** the outcome is `timeout` and the message is still delivered

### Requirement: No receive
The client SHALL subscribe to no topic. No message from the broker SHALL change a Graph, a sensor or a setting.

#### Scenario: Broker sends a message
- **WHEN** another client publishes to a topic below the base topic
- **THEN** the app ignores it and changes nothing

### Requirement: Password storage
The app SHALL store the broker password in `secrets.toml` in `$XDG_CONFIG_HOME/nicecooling/` with the mode 0600. The app SHALL never show the stored password. The password field SHALL show a placeholder when a password is set. The app SHALL never write the password to a log or to an export file. The daemon SHALL never see it.

#### Scenario: File mode
- **WHEN** the app writes `secrets.toml`
- **THEN** its mode is 0600

#### Scenario: Change the interval only
- **WHEN** the user changes the interval and leaves the password field untouched
- **THEN** the stored password stays

#### Scenario: Clear the password
- **WHEN** the user clears the password
- **THEN** the client connects with no password

### Requirement: TLS warning
The settings SHALL show a warning when TLS is off and a password is set. The warning SHALL not block the settings.

#### Scenario: Plain login
- **WHEN** TLS is off and a password is set
- **THEN** the settings show "The password goes over the network in clear text"

#### Scenario: TLS on
- **WHEN** TLS is on and a password is set
- **THEN** no warning shows

### Requirement: Broker settings screen
The app SHALL show the broker settings on the Settings screen, in a group "Network". It SHALL show the connection state: off, connecting, connected or failed with the last error text.

#### Scenario: Connected
- **WHEN** the client is connected
- **THEN** the screen shows "Connected"

#### Scenario: Failed
- **WHEN** the broker refuses the login
- **THEN** the screen shows "Failed" and the error text
