# Spike: the MQTT queue rules on `rumqttc`

Question: can the app keep the rules of the spec `mqtt-publishing` (one pending value for each sensor topic, a queue of 256, one response run for each acknowledgment, a queue that stays across reconnects) with the crate `rumqttc` and `rustls` (design G9 of `openspec/changes/add-node-graph`)?

Code: `tools/spikes/mqtt-queue` (outside the workspace, own `Cargo.lock` that starts from the root lock file).

- `src/broker.rs`: a test broker for MQTT 3.1.1 in the process, on a local port. It answers CONNECT, PUBLISH (QoS 0 and 1), SUBSCRIBE, PINGREQ and DISCONNECT. It stores what it receives. It can hold the PUBACK messages and release them in the reverse order, close a connection on a PUBLISH with no PUBACK, and stop and start again on the same port.
- `src/client.rs`: the pattern for the app (see "Pattern for the app").
- `src/main.rs`: the TLS setup (`tls_config`), a demo program and the tests.

## Result

Decision: go

All four tests pass: `pending_value_replaced_while_down`, `queue_of_256_drops_the_oldest`, `ack_maps_to_one_run` and `reconnect_keeps_the_queue`. The suite ran 5 times with no failure (7 tests, 0.69 s each time).

`rumqttc` alone gives none of the four behaviors. Code around it gives all four: the queue and the pending values live outside the client, and the client gets a message only while the connection is up. The spec needs no change.

## Crate and features

| Item | Value |
|---|---|
| `rumqttc` | `=0.25.1`, the newest release on crates.io on 2026-11-25 (`cargo search`, `cargo info`). License Apache-2.0 |
| Features | `default-features = false`, `features = ["use-rustls-no-provider"]` |
| `rustls` | `0.23.46`, `default-features = false`, `features = ["ring", "std", "tls12"]`. The same version as the root `Cargo.lock` |
| `rustls-native-certs` | `0.8.5`. The same version as the root `Cargo.lock` |
| `tokio` | `1.54.1`, features `rt-multi-thread`, `macros`, `net`, `time`, `io-util`, `sync` |

- The default feature `use-rustls` of `rumqttc` turns on `tokio-rustls/default`, and that pulls in `aws-lc-rs`. The feature `use-rustls-no-provider` has the same code without a provider. The app gives the provider itself: `tls_config` in `src/main.rs` builds the `ClientConfig` with `rustls::crypto::ring::default_provider()` and passes it with `Transport::tls_with_config(TlsConfiguration::Rustls(..))`. The test `tls_config_builds_with_the_ring_provider` builds it.
- `cargo tree --manifest-path tools/spikes/mqtt-queue/Cargo.toml -i rustls` shows one `rustls` (0.23.46), used by the spike and by `tokio-rustls` 0.26.6. The lock file has no `aws-lc` crate (`cargo tree | grep -c aws-lc` prints 0).
- `rumqttc` has its own dependency `rustls-webpki 0.102.8`. The lock file of the root workspace has `rustls-webpki 0.103.17` for `rustls`. The app will have both webpki versions. The `rustls` version stays one. Task 6.2 checks `cargo tree -p nicecooling -i rustls`, which is not affected.
- The spike reads the source of `rumqttc` 0.25.1 in the cargo registry (`src/eventloop.rs`, `src/state.rs`, `src/client.rs`) for the behaviors below. The tests measure them.

## The four results

### 1. One pending value for a sensor topic: `pending_value_replaced_while_down`

The broker is down. The test sets the value of one topic 100 times and of another topic 50 times. The client holds 2 slots and an empty queue. After the broker starts, it receives exactly 2 messages: the newest value of each topic (`a99`, `b49`), QoS 0, retain on. No other message comes in the next 300 ms.

`rumqttc` cannot do this alone. Its request channel (`flume::bounded`) has no way to replace or remove a message that waits. `rumqttc_alone_cannot_replace_or_drop_a_waiting_request` measures it: with a capacity of 4, four `try_publish` calls succeed, the fifth returns an error, and after the broker starts the four old messages go out in order. `publish().await` would wait for space instead of failing.

### 2. A queue of 256 drops the oldest: `queue_of_256_drops_the_oldest`

The broker is down. The test queues 300 response messages with the ids 0 to 299. The queue holds 256. The ids 0 to 43 get `Failed` at once, in order, each with a journal line (`eprintln!` in the spike, `tracing` in the app). After the broker starts, it receives the ids 44 to 299 in order, and the client reports `Ok` for each of the 256, once.

`rumqttc` cannot do this alone, for the same reason as result 1: the channel drops nothing.

### 3. An acknowledgment maps to one response run: `ack_maps_to_one_run`

The broker holds its PUBACK messages. The client sends the responses 10, 11 and 12 with 3 different packet ids. No report comes while the PUBACK messages are held. The broker then releases them in reverse order. The client reports `Ok` for 12, 11 and 10 in that order, one report for each id, and nothing more in the next 300 ms. The mapping follows the packet id and not the order of sending.

`rumqttc` gives no link from a publish to its acknowledgment:

- `AsyncClient::publish` returns `()`, and the packet id is assigned later, inside the event loop.
- The event loop emits `Event::Outgoing(Outgoing::Publish(pkid))` when it writes a packet (`pkid` 0 for QoS 0) and `Event::Incoming(Incoming::PubAck(pkid))` on the PUBACK.
- The code around it keeps a list of messages that it gave to the client, in the order it gave them. The event loop writes requests in the order of its channel, so each `Outgoing::Publish` event belongs to the first message of that list. The message then moves to a map keyed by `pkid`. The `PubAck` event removes it from the map and ends the response.

### 4. A reconnect keeps the queue: `reconnect_keeps_the_queue`

Part A: the broker is down and 5 messages wait. After the broker starts, it receives all 5 in order, and the client reports `Ok` for each.

Part B: the broker closes the connection on the first of 3 new messages (ids 6, 7 and 8), with no PUBACK. The client reconnects (the broker counts a second CONNECT). The message 6 reaches the broker at least twice. The client reports `Ok` exactly once for each of 6, 7 and 8, and the queue is empty at the end.

`rumqttc` alone loses the message. `rumqttc_alone_loses_the_unacked_publish_on_a_clean_reconnect` measures it: with a clean session, a publish with no PUBACK when the connection ends is not sent again after the reconnect, and the broker receives it once. The cause, in the source: when the connection fails, `EventLoop::clean` moves the packets in flight and the requests in the channel to `pending`, and the next `poll` clears `pending` when the CONNACK has `session_present` false, which is always true for a clean session. The spec needs a clean session, so this loss is part of the design.

## Pattern for the app

Copy `src/client.rs`. The rules:

1. The queue is not in `rumqttc`. The state is `Shared { slots, queue }` behind a mutex: `slots` maps a sensor topic to its newest payload and a "not sent" flag, and `queue` is a `VecDeque` of at most 256 messages with the fields `topic`, `payload`, `response_id` and a number `seq` for the order of arrival. The calls `set_value` and `enqueue` only change this state and wake the task. They never wait for the network. `enqueue` drops the oldest message at 256 and sends `Failed` for a response message.
2. One task owns the `EventLoop`. It pins the future `eventloop.poll()` and runs `select!` on `&mut poll` and a `Notify`. A wake-up pauses the future and never drops it, so the cancellation of `poll` is not a question. The task sets `up` on a successful `ConnAck`, and sets `up` off on any error from `poll`.
3. Hand-over happens only while `up`. The task calls `try_publish` for the queue first and for the slots after it, QoS 1 and no retain for the queue, QoS 0 and retain for the slots. It stops when 16 messages wait for an acknowledgment, which keeps the number below the capacity (32) of the request channel, so `try_publish` does not fail with "full". An `Outgoing::Publish` or `PubAck` event starts the hand-over again. After a `ConnAck`, every slot is marked "not sent", so the values go out at once after a connect.
4. The acknowledgment match: `sent` (a `VecDeque`, handed over and not yet written) and `inflight` (a `HashMap<pkid, message>`). Each `Outgoing::Publish(pkid)` pops the front of `sent` and, for `pkid != 0`, inserts it in `inflight`. Each `PubAck(pkid)` removes the entry and sends `Report { id, Ok }`. A response id gets one report because the entry leaves the map once.
5. On an error from `poll`, `inflight` and the QoS 1 part of `sent` go back to the front of the queue, sorted by `seq`, and the queue is cut to 256 again (the oldest go, as in result 2). The task waits and polls again, which reconnects. Nothing was handed over to the client while it was down, so no message sits in the request channel, and the clean-session reset of `rumqttc` has nothing to lose.
6. A message can reach the broker twice (result 4, part B): the broker may have kept the message and lost only the PUBACK. This is "at least once" delivery. A response run still ends once.

The app adds to this pattern: the backoff of task 6.5 in place of the wait of 50 ms, the 30 s limit of a response (a timer for each response message, which does not take the message out of the queue, as the spec says), the last will and the `online` message, the client id and the other settings, the `Failed` for a dropped response from `Shared` to the Graph service, and the clear of the queue when the host is cleared.

## Limits that the spike did not test

- TLS with a real broker. The spike builds the `ClientConfig` and does not connect over TLS. Task 6.6 has the TLS tests with `rcgen`.
- The 30 s limit and a late acknowledgment (the spec scenarios "Late acknowledgment" and "Slow publish"). The spike has no timer.
- The backoff, the last will, the status topic, the user name and password, and the keep-alive of 30 s on a silent line (the broker of the spike answers PINGREQ and never goes silent).
- A broker that acknowledges out of order with more than 16 messages in flight, and the packet id collision of `rumqttc` when `inflight` is full. The window of 16 is far below the default `inflight` of 100.
- A connection that drops between the write of a packet and the `Outgoing::Publish` event for it. The test closes the connection from the broker side with 1 of 3 messages, which covers an error that comes after a write. The order of events inside one `poll` result is read in the source of 0.25.1 and not tested for every case.
- A message with a payload larger than the packet limit of `rumqttc` (default 10 KiB for incoming and outgoing).
- Load: the spike ran 300 messages and a window of 16 on loopback. It did not measure the CPU use or the memory.
- The tests wait for a condition with a short poll of 5 ms (limit 10 s), and "nothing more arrives" is a wait of 300 ms. A slow machine can need a longer wait for the second kind.

## Other crates

| Item | Value |
|---|---|
| `lettre` | `=0.11.24`, the newest release on crates.io on 2026-11-25 (`cargo info lettre`). License MIT |
| Features | `default-features = false`, `features = ["builder", "smtp-transport", "tokio1", "tokio1-rustls", "ring", "rustls-native-certs"]` |

- `builder` gives `Message`. `smtp-transport` gives `AsyncSmtpTransport`. `tokio1` and `tokio1-rustls` give the `Tokio1Executor` and the TLS on `tokio-rustls`. `ring` is `rustls?/ring`: `lettre` builds its `ClientConfig` with the `ring` provider unless the app installed another provider. `rustls-native-certs` loads the system certificates for `CertificateStore::Default`.
- The feature `rustls-tls` is not used: it adds `webpki-roots`, a fixed copy of the Mozilla roots, and the spec asks for the system certificates. The default features are off, so `native-tls`, `pool` and `hostname` are off too. The EHLO name is the default of `lettre`.
- `lettre` depends on `rustls` `0.23.27` with `default-features = false`, and on `tokio-rustls` `0.26` with `default-features = false`. The lock file keeps one `rustls` (0.23.46) and one `tokio-rustls` (0.26.6), shared with `rumqttc`. `cargo tree -p nicecooling -i rustls` shows one `rustls` version, and `cargo tree -p nicecooling | grep -c aws-lc` prints 0.
- The new packages in the lock file are `lettre`, `email-encoding`, `email_address`, `httpdate` and `quoted_printable`. `cargo tree -p nicecoolingd` lists no `lettre` and no `rumqttc`.
