# Graph sensor and Curve check

This procedure checks that a Graph sensor drives the Curve of a Channel on the reference Board. A Graph gives the larger of two temperatures. The Curve of one Channel reads that Graph sensor. The check also covers a source with no value, and the stop of the app. Each step has a pass line.

## Safety

- In steps 6 and 8 the Channel `nct6798_pwm1` goes to 100% for some seconds. Its fan runs at full speed and is loud. Use a case fan on this Channel.
- Do not use the Pump channel `nct6798_pwm6`. The check changes the smoothing and the step limit of the Curve, and step 9 puts the old values back.
- The check needs no root and writes no Leakshield setting.

## Preconditions

- Fedora 45 with GNOME on the reference desktop. The package files are installed (`docs/app.md`, Install). The unit `nicecoolingd` runs.
- The app runs: `pgrep -a nicecooling` prints one process.
- The Channel `nct6798_pwm1` has a fan. It is not the Pump channel of a Loop.
- The Graphs screen is in the group "Control" of the sidebar. The Sensors screen shows the group "Graph sensors".
- A terminal for `busctl`. It stays open during the check.

## Helper

Run this block once in the terminal:

```bash
B=io.github.cappyt.NiceCooling1
R=/io/github/cappyt/NiceCooling1
CH=nct6798_pwm1
duty() { busctl get-property $B $R/channel/$CH $B.Channel Duty; }
alarms() { busctl get-property $B $R $B.Alarms Active; }
```

`duty` prints `d` and the Duty of the Channel in percent. `alarms` prints each active Alarm and Warning: instance id, event id, severity, source, response, raised time, poweroff time, details.

## The Curve

The Curve has two points: 30 °C gives 20% and 60 °C gives 80%. The Curve interpolates in a straight line between the points. So it gives these values:

| Input | Duty |
|---|---|
| 40 °C | 40% |
| 50 °C | 60% |

A Duty that differs by 1 from the table is correct: the Channel works in steps of 1/255.

## Steps

1. Write down the old settings of the Channel. Open the Channels screen and select `nct6798_pwm1`. Write down the Control, the Input, the Input smoothing, the Duty step limit and each point of the Curve. Run `duty`. Expected: `duty` prints `d` and a number. The old settings are on paper.
2. Make the two source Graphs. On the Graphs screen press "New sensor". The editor opens with a sensor end node. Add a constant node: type `constant` in the search field of the node list and double click the result. Set its value to 30. In the panel of the sensor end node, section "Inputs", select the constant node for the pin `a`. Set the unit of the end node to `degC` and the name of the Graph and of the sensor to `Source A`. Press "Apply". On the Graphs screen switch the Graph on. Make a second Graph in the same way: the constant is 40 and the name is `Source B`. Expected: within 3 s the Sensors screen, group "Graph sensors", shows "Source A" with 30 °C and "Source B" with 40 °C.
3. Make the Graph that gives the larger value. Press "New sensor". Add two sensor source nodes. Set the sensor of the first to "Source A" and the sensor of the second to "Source B". Add a maximum node. In the panel of the maximum node, section "Inputs", select the first source for the pin `a` and the second source for the pin `b`. In the panel of the sensor end node select the maximum node for the pin `a`. Set the unit `degC` and the name `Larger`. Press "Apply" and switch the Graph on. Expected: within 3 s the Sensors screen shows "Larger" with 40 °C. In the editor the first source shows 30, the second source shows 40, and the maximum node shows 40.
4. Set the Curve. Open the Channels screen and select `nct6798_pwm1`. Set Control to "Curve" and Input to "Larger". Set "Input smoothing" to 0 and "Duty step limit" to 100. Use "Add point" and "Remove point" so that the Curve has two points: Input 30 with Duty 20, and Input 60 with Duty 80. Expected: the editor shows the text "A Graph sensor has no value while the app does not run. The Channel then runs at 100%." Within 3 s `duty` prints `d 40`, with a difference of 1 at most.
5. Raise a source by 10 °C. On the Graphs screen press "Edit" on "Source B". Set the value of the constant node to 50. Press "Apply". Wait 2 s. Run `duty`. Expected: `duty` prints `d 60`, with a difference of 1 at most. The Sensors screen shows "Source B" and "Larger" with 50 °C.
6. Remove a source. On the Graphs screen switch "Source B" off. Run `date +%s`, then wait 5 s and run `duty` and `alarms`. Expected: the Sensors screen shows `--` for "Source B" and for "Larger", and never a number. `duty` prints `d 100`. `alarms` shows one entry with the event `lost-input`, the severity `warning` and the source `nct6798_pwm1`. The Alarms screen shows the Warning "Input lost" in the active list.
7. Give the source back. Switch "Source B" on. Wait 5 s. Run `duty` and `alarms`. Expected: `duty` prints `d 60`, with a difference of 1 at most. `alarms` shows no `lost-input` entry, and the Warning is gone from the Alarms screen.
8. Close the app. Open the tray menu and press "Quit". Run `date +%s`, then wait 5 s and run `pgrep -a nicecooling`, `duty` and `alarms`. Expected: `pgrep` prints nothing. `duty` prints `d 100`. `alarms` shows one `lost-input` entry with the source `nct6798_pwm1`.
9. Start the app again and clean up. Run `nicecooling --background &`. Wait 10 s. Expected: `duty` prints `d 60` with a difference of 1 at most, and `alarms` shows no `lost-input` entry. Then open the Channels screen and put the old settings of step 1 back. On the Graphs screen delete "Larger", then "Source A" and "Source B". Expected: each Graph leaves the list. If a delete fails, the row names the Curve that still uses the Graph sensor.

## Record

Date, Board, kernel version:

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
| 9 | |
