# Leakshield: Settings tab (wizard)

Comp: `../comps/leakshield-wizard.html` (step 4, 1280x800; also 1100x700 in `../comps/shots/`).

The Settings tab is a wizard with seven steps. The user can also open one step directly to change one setting.

## Layout

1. Stepper card: Reservoir, Pump, Flow, Pressure, Standby, Mode, Summary. A done step has a check. A click on a step opens it.
2. Step card: title, "Step 4 of 7", one row per setting, then the footer with "Cancel" at the left and the back and next buttons at the right. The back and next buttons carry the name of their step.
3. "Pending changes" card, under the step card: each changed setting with its old and new value, and the write path.

Each setting row has: the name, the verification badge, a caption, and the control at the right.

## Steps

| Step | Settings |
|---|---|
| Reservoir | "Reservoir": select with the known models and "Custom". With "Custom": "Total volume" (ml), "Air volume at maximum fill level" (ml), "Air volume at minimum fill level" (ml). "Automatic fill-level measurement": switch, with a warning caption |
| Pump | "Pump": select with the known types and "Custom". With "Custom": "Maximum pressure" (mbar), "Maximum speed" (rpm). "Speed source": read-only text "Pump channel of Main loop: Waterpump" with the link "Open Loop" |
| Flow | "Flow source": read-only text "Flow sensor of Main loop: Water Flow, 115 L/h" with "Open Loop". Without a flow sensor: "None. The Leakshield uses the pump speed only." |
| Pressure | "Adaptive pressure": switch. "Expert values": minimum, target, maximum in mbar, used only with adaptive pressure off. "Time-controlled vacuum generation": switch and two times |
| Standby | "Default pump speed" (rpm) and "Default flow" (L/h), each with the button "Use the current value". "Reduced vacuum in standby": switch and a value in mbar |
| Mode | The three Modes as radio rows with their captions. RELEASE has the padlock |
| Summary | The diff table (see `../widgets.md`), notes and warnings above it, and the button "Apply 5 changes" |

## States

| State | What the user sees |
|---|---|
| Empty | No pending change: the "Pending changes" card says "No change yet." The Summary says "Nothing to apply." and "Apply" is disabled |
| Loading | "Reading the settings" with a spinner in the step card |
| Device offline | Offline banner. Controls are disabled. Pending changes stay |
| Read-only field | Dashed field, padlock, "Read-only", and the reason: "Not a Verified field: this Safety field has no Verification write yet." with "Open Fields" |
| Unknown firmware | Banner 3 of the shell. Every field is read-only. Per-field reasons are hidden |
| Locked | RELEASE in the Mode step. Apply then opens the polkit dialog |
| Value out of range | The input has a red outline. Caption: "80 to 450 mbar." The next button stays active, and the Summary lists the error and disables "Apply" |
| Applying | Each row of the diff table shows its write status. "Apply" becomes "Applying 2 of 5" |
| Write failed | The row has the `alarm-tint` ground and the text "Write failed: the read-back does not match." The writes stop. Button "Restore the backup" with the padlock, and "Keep the current state" |
| Applied | Success row: "5 settings written and verified." The changes go into the active Profile |
| Alarm | The poweroff banner shows above the wizard. The wizard stays open |
| Maintenance pause | Banner 4 of the shell. The wizard still writes, because the user starts the write |

## Interactions

- The wizard writes nothing before "Apply". "Cancel" discards the pending changes after a confirmation: "Discard 2 changes?"
- Leaving the tab with pending changes keeps them. The tab label then has a dot.
- Summary warnings come from the settings: "MONITOR does not prevent a leak.", "The pump pressure is above 470 mbar: the Leakshield can only work as a leak monitor.", "Automatic fill-level measurement releases the pressure for a short time."
- Left and Right arrow keys do not change the step. Alt+Left and Alt+Right do.

## Copy

| Place | Text |
|---|---|
| Adaptive pressure | "The Leakshield computes the pressure from the pump speed and the flow of the Feed. Recommended." |
| Expert values | "Not used while adaptive pressure is on." |
| Time-controlled | "The Leakshield builds the negative pressure at two times of the day, so that you choose when its pump makes noise." |
| Automatic measurement | "The Leakshield measures the fill level after a change, for example when you open the reservoir. During the measurement the Loop has no leak protection." |
| Standby | "After standby the PC sends no Feed yet. The Leakshield then uses these values." |
| Pending card | "The wizard writes nothing before you select Apply on the Summary step. Then each write has four steps: Backup saved, Written, Read back, Verified: only the expected bytes changed." |
