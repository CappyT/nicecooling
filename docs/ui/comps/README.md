# Comps

Static HTML references for the GPUI build. They show layout, tokens and states. They are not product code.

| File | Screen and state |
|---|---|
| `overview.html` | Overview with a large leak Alarm and the poweroff banner |
| `overview-min.html` | Overview at 1100x700: two Loops, one without a Leakshield, one out-of-range flag |
| `daemon-down.html` | Overview with the daemon down: red banner, values not live |
| `countdown-dialog.html` | Poweroff countdown dialog over the Overview |
| `channels.html` | Channels with the Curve editor on a Pump channel |
| `leakshield-overview.html` | Leakshield, Overview tab, normal state |
| `leakshield-status.html` | Leakshield, Status tab with the device schematic |
| `leakshield-wizard.html` | Leakshield, Settings wizard, step 4 Pressure, with one read-only field |
| `profiles.html` | Profiles with the rule editor open and the Default Profile in the list |

Shared files: `nc.css` has every token and component style. `nc.js` builds the shell, the icons and the Loop diagram, so that each comp stays short. `it.js` has Italian strings for the layout check.

## Variants

Add a query to the file URL:

| Query | Effect |
|---|---|
| `?theme=dark` or `?theme=light` | Forces the theme. Without it the comp follows the system |
| `&lang=it` | Italian strings (`channels.html`, `profiles.html`, `leakshield-overview.html`) |

Open a comp at 1280x800, or at 1100x700 for the minimum window. The layout is the same at both sizes.

## Renders

`shots/` has a PNG of each comp in light and dark. Suffixes: `-min` is 1100x700, `-it` has Italian strings.

Every value in the comps is synthetic.
