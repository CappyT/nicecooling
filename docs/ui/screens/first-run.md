# First run

No comp. The stepper and the step card follow `../comps/leakshield-wizard.html`. The Loop step shows the Loop diagram of `../comps/overview.html` as a live preview.

The first-run flow opens in the window when the daemon has no Loop. The user can leave it at any step and open it again from the empty Overview.

## Steps

| Step | Content |
|---|---|
| 1. Board | The detection result: Board name, the Board file in use, the count of Channels and sensors. A table of the Channels with name, rpm and Duty |
| 2. Loop | Four pickers: "Pump channel", "Flow sensor" with "Calibration table preset", "Water temperature", "Leakshield". Under them, the Loop diagram of the choices, live. Then "Pump floor" with its default of 40% |
| 3. BIOS snapshot | An explanation with the date of the snapshot ("Saved 2026-11-20 08:14") and a table of the Hardware curves that it holds: one row per Channel, with its auto points as "30 °C: 20 %". A Channel without auto points shows "--" |
| 4. Leakshield | Only with a Leakshield: serial, firmware version, the count of Verified fields and read-only fields, and what to do next |
| 5. Done | A summary and the buttons "Open the Overview" and "Open the Leakshield wizard" |

Footer of each step: "Skip setup" at the left, the back and next buttons at the right.

## States

| State | What the user sees |
|---|---|
| Daemon offline | The flow cannot start: "nicecooling needs its daemon. Start it: systemctl start nicecoolingd" with "Try again" |
| Board with a Board file | "ASUS ROG CROSSHAIR VIII DARK HERO. Board file found: Channels and sensors have their Board names." |
| Board without a Board file | "No Board file for this Board. Channels and sensors have the names of the kernel. You can rename them in Channels and Sensors." |
| No Channel found | Warning: "No Channel found. nicecooling needs a hwmon driver for the fan controller of this Board." The Loop step is disabled. "Skip setup" stays |
| No flow sensor | The picker has "None". The diagram has no flow sensor part |
| Flow preset | With a flow sensor: select "Calibration table preset": "Alphacool 17558", "Empty table". The live flow shows next to it: "671 rpm = 115 L/h" |
| Empty table | The flow sensor is not linked to the Loop. A note under the picker: "The flow sensor has no value until its Calibration table has 2 points. Build the table on the Loops screen." |
| No Leakshield | The picker has only "None". Step 4 is skipped. Text: "Without a Leakshield, the Loop has its Pump floor and its pump-stop Alarm." |
| Leakshield with known firmware | "Firmware 1022: known. 96 of 112 fields are Verified fields. 16 Safety fields are read-only until a Verification write passes." Button "Open Fields" |
| Leakshield with unknown firmware | Warning: "Firmware 1042 is unknown. nicecooling shows the Leakshield and cannot change its settings." |
| Loading | "Reading the Board" with a spinner in step 1 |
| Locked | Not used |
| Alarm | The poweroff banner and the dialog show above the flow |

## Interactions

- "Next" in the Loop step creates the Loop. The step needs a Pump channel.
- "Skip setup" closes the flow and opens the Overview. The empty state there offers "Set up a Loop".
- The flow never writes the Leakshield.

## Copy

| Place | Text |
|---|---|
| BIOS snapshot text | "Before nicecooling controls a Channel, it saves the BIOS snapshot: the Hardware curves that your Board had. When the daemon stops, and also after a crash, nicecooling writes the snapshot back. If the kernel hangs, no program runs, and each Channel stays at its last Duty." |
| Channels without a Curve | "A Channel without a Curve stays on its Hardware curve. nicecooling does not touch it." |
| Leakshield text | "nicecooling writes only Verified fields. A low-risk field is verified by a plausible read. A Safety field needs a Verification write. That is a supervised procedure outside the app: see the project documentation." |
| Done | "The Loop is set up. Its Channels still run their Hardware curves: add a Curve in Channels." |
