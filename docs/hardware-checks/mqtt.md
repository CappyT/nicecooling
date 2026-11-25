# MQTT check

This procedure checks the MQTT client of the app against a real broker with TLS and a user name. The client has a keep-alive of 30 s. A subscriber on `<base>/#` shows what the client publishes: the status, the sensor values, the Alarm events and the behavior when the broker stops. Each step has a pass line.

## Safety

- Steps 5 and 7 raise a real pump-stop Alarm on a test Loop. All Channels run at 100% while the Alarm is active. The check sets the alarm response of `pump-stop` to "notify" first, so that no poweroff starts. Step 8 puts the old response back.
- Step 7 stops the broker for 2 minutes. Do not run the check on a broker that other clients need.
- Step 6 stops the app with `kill -9`.

## Preconditions

- Fedora 45 with GNOME on the reference desktop. The package files are installed (`docs/app.md`, Install). The unit `nicecoolingd` runs. The app runs.
- A broker with TLS and a user name and password. `<host>` is its name and `<port>` is its TLS port, usually 8883. You can stop and start the broker with `sudo systemctl stop mosquitto` and `sudo systemctl start mosquitto` on its host.
- `mosquitto_sub` on this PC (`sudo dnf install mosquitto`).
- A fan on the spare header `nct6798_pwm4`, as in `docs/hardware-checks/alarms.md`. You can unplug it and plug it in.
- The base topic is `nicecooling`. It is written `<base>` below.

## Helper

Run this block once in each terminal that the steps name. Set `BROKER`, `CAFILE`, `MQUSER` and `MQPASS` first. For a broker with a public certificate, `CAFILE` is `/etc/pki/tls/certs/ca-bundle.crt`.

```bash
BROKER=<host>
PORT=<port>
CAFILE=/etc/pki/tls/certs/ca-bundle.crt
MQUSER=<user>
MQPASS=<password>
mq() { mosquitto_sub -h "$BROKER" -p "$PORT" --cafile "$CAFILE" -u "$MQUSER" -P "$MQPASS" "$@"; }
B=io.github.cappyt.NiceCooling1
R=/io/github/cappyt/NiceCooling1
BC="busctl --allow-interactive-authorization=true"
SPARE=nct6798_pwm4
```

Each line that the subscriber prints has the form `<ISO time> <QoS> <retain flag> <topic> <payload>`. The retain flag is 1 for a retained message.

## Steps

1. Start the subscriber. In terminal 1 run the helper, then `while true; do mq -t 'nicecooling/#' -F '%I %q %r %t %p'; sleep 1; done`. Expected: the command stays open. It prints no line yet, or only old retained lines.
2. Set the broker in the app. Open Settings, group "Network". Set the host `<host>`, the port `<port>`, TLS on, the user name, the password and the base topic `nicecooling`. Set the interval to 5 s. Expected: the MQTT state shows "Connected" within 10 s. The password field shows a placeholder and never the password. In terminal 1 the line `nicecooling/status online` shows with QoS 1 and retain flag 1. In another terminal `stat -c %a ~/.config/nicecooling/secrets.toml` prints `600`.
3. Check the sensor messages. Look at terminal 1 for 20 s. Then run the helper in terminal 2 and run `mq -t nicecooling/sensor/asusec_temp4 -C 1 -W 5 -F '%r %p'`, wait 6 s and run it again. Expected: in terminal 1 each sensor has one line about every 5 s, with QoS 0 and retain flag 1, and the payload has `value`, `unit`, `name` and `time`. Each run in terminal 2 prints at once a line that starts with `1`, which is a retained message. The `time` of the second line is 4 to 7 s later than the `time` of the first line, because `time` counts in microseconds.
4. Check a sensor with no value. On the Graphs screen press "New sensor". Add a constant node with the value 20. In the section "Inputs" of the sensor end node select the constant node for the pin `a`. Set the unit `degC` and the name `MQTT probe`. Press "Apply" and switch the Graph on. Expected: within 10 s terminal 1 shows a line for `nicecooling/sensor/graph_<n>` with `"value":20` and the name "MQTT probe". Write down the number `<n>`. Switch the Graph off. Expected: within 10 s the next line for that topic has `"value":null`.
5. Raise and close a pump-stop test event. Run the helper in terminal 2. Run `$BC call $B $R $B.Manager CreateLoop s "MQTT test"`. Write down the last element of the returned path as `TL`. Run `$BC call $B $R/loop/$TL $B.Loop SetPumpChannels ao 1 $R/channel/$SPARE`. Run `busctl call $B $R $B.Alarms GetResponses | rg -o '"pump-stop" "[a-z]+" [0-9]+'` and write down the old response (the default is `"poweroff" 30`). Run `$BC call $B $R $B.Alarms SetResponse ssu pump-stop notify 0`. Unplug the fan of the spare header. Run `$BC call $B $R/channel/$SPARE $B.Channel SetFixedDuty d 50`. Wait 15 s. Expected: terminal 1 shows one line for `nicecooling/event/pump-stop`, with QoS 1, retain flag 0, `"state":"raised"`, `"severity":"alarm"` and `"source":"<TL>"`. Plug the fan in. Wait 20 s. Expected: terminal 1 shows one more line for the same topic with `"state":"cleared"` and the same `source` and `details`. No other line shows for that topic.
6. Stop the app without a clean disconnect. In terminal 2 run `kill -9 $(pgrep -x nicecooling)` and write down the time. Expected: within 45 s terminal 1 shows `nicecooling/status offline` with QoS 1 and retain flag 1. Run `nicecooling --background &`. Expected: within 10 s terminal 1 shows `nicecooling/status online` again.
7. Stop the broker. On the Graphs screen switch the Graph `MQTT probe` on and check that its live result is 20. Run `sudo systemctl stop mosquitto` on the broker host. Write down the time. Wait 10 s. Unplug the fan of the spare header. Run `$BC call $B $R/channel/$SPARE $B.Channel SetFixedDuty d 50`. Wait until 2 minutes have passed since the stop. Expected: during the stop the Graphs screen keeps showing the live result 20 for `MQTT probe`, and the row shows no error. The Settings screen shows the MQTT state "Connecting" or "Failed". Run `sudo systemctl start mosquitto` on the broker host. Expected: within 70 s terminal 1 shows `nicecooling/status online` and one line for `nicecooling/event/pump-stop` with `"state":"raised"`. The `time` of that line is a time between the stop and the start of the broker. Plug the fan in. Wait 20 s. Expected: terminal 1 shows one line for `nicecooling/event/pump-stop` with `"state":"cleared"`.
8. Clean up. Run `$BC call $B $R/loop/$TL $B.Loop Delete`. Run `$BC call $B $R $B.Alarms SetResponse ssu pump-stop poweroff 30` with the old response of step 5. On the Graphs screen delete the Graph `MQTT probe`. In Settings, group "Network", clear the host. Stop terminal 1 with Ctrl+C. Expected: the MQTT state shows "Off", and the app opens no connection to the broker (`ss -tn dst <host>` prints no connection of the app).

## Record

Date, broker and version:

| Step | Result |
|---|---|
| 1 | |
| 2 | |
| 3 | |
| 4 | |
| 5 | |
| 6 | |
| 7 | |
| 8 | |
