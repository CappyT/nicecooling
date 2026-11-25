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
| `graphs.html` | Graphs: a running Graph with a sensor end node, a fired response, a disabled Graph, an invalid Graph and a Graph sensor without a Graph |
| `graph-editor.html` | Graph editor on a Graph with a sensor end node: node list, canvas with live values, one draft-only node, parameter panel of a Limiter with its band guide |
| `graph-editor-response.html` | Graph editor on a Graph with a response end node: response panel with the poweroff warning and the last runs, and a try with its banner |
| `settings-network.html` | Settings, card "Network" with both warnings, and the Response pause banner of the shell |

Shared files: `nc.css` has every token and component style. `nc.js` builds the shell, the icons, the Loop diagram and the Graph canvas, so that each comp stays short. `it.js` has Italian strings for the layout check.

## Variants

Add a query to the file URL:

| Query | Effect |
|---|---|
| `?theme=dark` or `?theme=light` | Forces the theme. Without it the comp follows the system |
| `&lang=it` | Italian strings (`channels.html`, `profiles.html`, `leakshield-overview.html`, `graphs.html`, `graph-editor.html`, `graph-editor-response.html`, `settings-network.html`) |

Open a comp at 1280x800, or at 1100x700 for the minimum window. The layout is the same at both sizes.

## Renders

`shots/` has a PNG of each comp in light and dark. Suffixes: `-min` is 1100x700, `-it` has Italian strings.

A render is a capture of the local file by a headless Chromium, with no server and no display. For `channels-min-it-dark.png`:

`chrome-headless-shell --disable-gpu --hide-scrollbars --force-color-profile=srgb --virtual-time-budget=2000 --window-size=1100,700 --screenshot=shots/channels-min-it-dark.png "file://$PWD/channels.html?theme=dark&lang=it"`

`overview-min.html` has no `-min` variant of its own: its renders are 1100x700.

Every value in the comps is synthetic.
