# MQTT publishing

## Purpose

Defines the MQTT client of the daemon: the broker settings, the connection and reconnect rules, the bounded queue, the topics with their payload, QoS and retained flag, and the storage of the password. The client only publishes. It never subscribes.

## ADDED Requirements

### Requirement: Broker settings
The daemon SHALL hold one broker: host, port, TLS on or off, an optional CA file, user name, password, client id, base topic and value interval. The port SHALL default to 1883, or 8883 with TLS. The client id SHALL default to `nicecooling-<hostname>`. The base topic SHALL default to `nicecooling`. The value interval SHALL be 1 s to 1 h, default 10 s.

#### Scenario: Defaults
- **WHEN** the user sets only the host `broker.lan` and TLS on
- **THEN** the port is 8883, the client id is `nicecooling-<hostname>`, the base topic is `nicecooling` and the interval is 10 s

#### Scenario: Interval above the limit
- **WHEN** a write has the value interval 7200 s
- **THEN** the daemon refuses it with `InvalidArgument`

#### Scenario: Wildcard in the base topic
- **WHEN** a write has the base topic `home/#`
- **THEN** the daemon refuses it with `InvalidArgument`

### Requirement: MQTT off until configured
MQTT SHALL be off while the host is empty. In that state the daemon SHALL open no connection and SHALL queue nothing. A change of the host or of any setting SHALL close the connection and open a new one with the new settings.

#### Scenario: Fresh install
- **WHEN** no host is set
- **THEN** the daemon makes no network connection for MQTT

#### Scenario: Host cleared
- **WHEN** the user clears the host while connected
- **THEN** the daemon closes the connection and drops the queue

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

### Requirement: No block of the tick
The MQTT client SHALL run in its own task. A broker that is unreachable or slow SHALL never delay the control tick, a Graph evaluation or the watchdog ping.

#### Scenario: Broker unreachable
- **WHEN** the broker host does not answer
- **THEN** the control tick still runs once per second and the watchdog still gets its ping

#### Scenario: Slow publish
- **WHEN** the broker takes 20 s to accept a message
- **THEN** no tick runs late

### Requirement: Bounded queue
The client SHALL hold at most 256 messages in memory. When the queue is full it SHALL drop the oldest sensor value message first. When the queue holds only other messages it SHALL drop the oldest message. A dropped event or action message SHALL give the outcome `failed` to its action, and SHALL write a line to the journal. The queue SHALL stay across reconnects.

#### Scenario: Values dropped first
- **WHEN** the queue holds 256 messages, 200 of them sensor values, and an event message arrives
- **THEN** the oldest sensor value message is dropped and the event message waits

#### Scenario: Event waits
- **WHEN** an event message is queued while the broker is down for 2 minutes
- **THEN** the client sends it after the reconnect

#### Scenario: Queue of events only
- **WHEN** the queue holds 256 event messages and another arrives
- **THEN** the oldest event message is dropped

### Requirement: Status topic
The client SHALL publish `online` to `<base>/status` with QoS 1 and the retain flag after each connect. It SHALL set the last will `offline` on the same topic with QoS 1 and the retain flag.

#### Scenario: Connect
- **WHEN** the client connects
- **THEN** `<base>/status` holds `online`, retained

#### Scenario: Daemon dies
- **WHEN** the daemon stops without a clean disconnect
- **THEN** the broker publishes `offline` on `<base>/status`

### Requirement: Sensor topics
For each sensor that the user has not hidden, the client SHALL publish one JSON object to `<base>/sensor/<sensor id>` at each value interval and at once after a connect, with QoS 0 and the retain flag. The object SHALL have `value`, `unit`, `name` and `time`. `time` SHALL be microseconds since the Unix epoch.

#### Scenario: Value message
- **WHEN** the sensor `nct6798_temp1` named "CPU" reads 46.2 °C
- **THEN** the topic `nicecooling/sensor/nct6798_temp1` gets an object with `value` 46.2, `unit` "°C" and `name` "CPU"

#### Scenario: Hidden sensor
- **WHEN** the user hides a sensor
- **THEN** the client publishes no message for it at the next interval

#### Scenario: Graph sensor
- **WHEN** a Graph sensor `graph_3` exists
- **THEN** the client publishes it like any other sensor

### Requirement: Sensor without a value
A sensor that has no value SHALL publish an object with `value` set to `null`, so the reader sees the loss. A sensor of a disabled Graph SHALL do the same.

#### Scenario: Lost sensor
- **WHEN** a sensor has had no reading for more than 3 s
- **THEN** its next message has `value` null

#### Scenario: Disabled Graph
- **WHEN** the user disables a sensor Graph
- **THEN** its next message has `value` null

### Requirement: Event topics
The client SHALL publish one JSON object to `<base>/event/<event id>` for each raise and each clear of an Alarm or Warning, with QoS 1 and no retain flag. The object SHALL have `event`, `source`, `severity` (`alarm` or `warning`), `state` (`raised` or `cleared`), `time` and `details`. `details` SHALL hold the keys of the event, such as `value`, `limit` and `unit`, when they apply.

#### Scenario: Raise
- **WHEN** the pump-stop Alarm of `loop_1` rises
- **THEN** `nicecooling/event/pump-stop` gets an object with `source` `loop_1`, `severity` `alarm` and `state` `raised`

#### Scenario: Clear
- **WHEN** that Alarm closes
- **THEN** the topic gets an object with `state` `cleared`

#### Scenario: Event id with a dot
- **WHEN** the event `leakshield.leak-alarm` rises
- **THEN** the topic is `nicecooling/event/leakshield.leak-alarm`

### Requirement: Action message topic
The message of an MQTT action node SHALL use its own topic below the base topic, QoS 1 and no retain flag, as the graph-actions spec states. The daemon SHALL give the action its outcome when the broker acknowledges the message, when the queue drops it, or after 30 s without an answer.

#### Scenario: Acknowledged
- **WHEN** the broker acknowledges the message after 1 s
- **THEN** the outcome of the action is `ok`

#### Scenario: Late acknowledgment
- **WHEN** the broker acknowledges after 40 s
- **THEN** the outcome is `timeout` and the message is still delivered

### Requirement: No receive
The client SHALL subscribe to no topic. No message from the broker SHALL change a Graph, a sensor or a setting.

#### Scenario: Broker sends a message
- **WHEN** another client publishes to a topic below the base topic
- **THEN** the daemon ignores it and changes nothing

### Requirement: Password storage
The daemon SHALL store the broker password in a file of the state directory that only root reads, mode 0600. The D-Bus API SHALL never return the password. A read SHALL return a flag that says whether a password is set. A write without a password argument SHALL keep the stored password, and an empty password SHALL clear it. The daemon SHALL never write the password to the journal.

#### Scenario: Read settings
- **WHEN** a client reads the broker settings
- **THEN** the reply holds the host, port, TLS, CA file, user name, client id, base topic, interval and the flag, and no password

#### Scenario: Change the interval only
- **WHEN** the user changes the interval and sends no password
- **THEN** the stored password stays

#### Scenario: Clear the password
- **WHEN** the user sends an empty password
- **THEN** the flag reads false and the client connects with no password

### Requirement: Broker settings authorization
A write of the broker settings SHALL need the polkit action `io.github.cappyt.NiceCooling1.write`. A read SHALL need no polkit action.

#### Scenario: Write by a wheel member
- **WHEN** a `wheel` member in the active session sets the host
- **THEN** the call succeeds under the rule of `write`

#### Scenario: Read by any user
- **WHEN** a user outside `wheel` reads the settings
- **THEN** the call succeeds and the reply holds no password

### Requirement: Broker settings screen
The app SHALL show the broker settings on the Settings screen, in a group "Network". It SHALL show the connection state: off, connecting, connected or failed with the last error text. The password field SHALL show a placeholder when a password is set and SHALL not show the value.

#### Scenario: Connected
- **WHEN** the client is connected
- **THEN** the screen shows "Connected"

#### Scenario: Password set
- **WHEN** a password is set
- **THEN** the field shows a placeholder of dots and never the stored text

#### Scenario: Daemon offline
- **WHEN** the daemon is unreachable
- **THEN** the screen shows the offline banner and turns the write controls off
