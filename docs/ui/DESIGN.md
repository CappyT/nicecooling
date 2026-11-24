---
name: nicecooling
description: A GNOME system tool for a water-cooled PC, with each Loop drawn as a piping diagram.
colors:
  window: "#FAFAFB"
  view: "#FFFFFF"
  sidebar: "#EBEBED"
  card: "#FFFFFF"
  headerbar: "#FFFFFF"
  fg: "#2A2A2E"
  fg-dim: "#5E5E66"
  border: "#DCDCE0"
  border-strong: "#B8B8C0"
  hover: "#F0F0F2"
  selected: "#D9D9DD"
  track: "#E4E4E8"
  accent-bg: "#1C71D8"
  accent-fg: "#FFFFFF"
  accent: "#0461BE"
  accent-tint: "#E3EEFB"
  alarm-bg: "#C01C28"
  alarm-fg: "#FFFFFF"
  alarm: "#C01C28"
  alarm-tint: "#FBE4E5"
  warning-bg: "#E5A50A"
  warning-fg: "#2A2A2E"
  warning: "#905400"
  warning-tint: "#FDF3D9"
  success: "#1B8553"
  ok-dot: "#2EC27E"
  offline: "#6B6A70"
  experimental: "#8939A4"
  experimental-tint: "#F3E6F6"
  water: "#1C71D8"
  air: "#77767B"
  part: "#F6F5F4"
typography:
  caption:
    fontFamily: "Adwaita Sans, Inter, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: "16px"
  body:
    fontFamily: "Adwaita Sans, Inter, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: "20px"
  heading:
    fontFamily: "Adwaita Sans, Inter, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 700
    lineHeight: "20px"
  value:
    fontFamily: "Adwaita Sans, Inter, system-ui, sans-serif"
    fontSize: "16px"
    fontWeight: 700
    lineHeight: "22px"
  title:
    fontFamily: "Adwaita Sans, Inter, system-ui, sans-serif"
    fontSize: "18px"
    fontWeight: 700
    lineHeight: "24px"
  numeral:
    fontFamily: "Adwaita Sans, Inter, system-ui, sans-serif"
    fontSize: "28px"
    fontWeight: 800
    lineHeight: "32px"
  countdown:
    fontFamily: "Adwaita Sans, Inter, system-ui, sans-serif"
    fontSize: "72px"
    fontWeight: 800
    lineHeight: "76px"
  tag-mono:
    fontFamily: "Adwaita Mono, Source Code Pro, monospace"
    fontSize: "12px"
    fontWeight: 700
    lineHeight: "16px"
rounded:
  sm: "6px"
  control-sm: "7px"
  control: "9px"
  card: "12px"
  dialog: "15px"
  pill: "999px"
spacing:
  xs: "3px"
  sm: "6px"
  md: "12px"
  lg: "18px"
  xl: "24px"
  xxl: "36px"
components:
  button:
    backgroundColor: "{colors.selected}"
    textColor: "{colors.fg}"
    rounded: "{rounded.control}"
    height: "34px"
    padding: "0 15px"
  button-suggested:
    backgroundColor: "{colors.accent-bg}"
    textColor: "{colors.accent-fg}"
    rounded: "{rounded.control}"
    height: "34px"
  button-destructive:
    backgroundColor: "{colors.alarm-bg}"
    textColor: "{colors.alarm-fg}"
    rounded: "{rounded.control}"
    height: "34px"
  card:
    backgroundColor: "{colors.card}"
    rounded: "{rounded.card}"
  banner-alarm:
    backgroundColor: "{colors.alarm-bg}"
    textColor: "{colors.alarm-fg}"
    height: "64px"
  tag:
    backgroundColor: "{colors.selected}"
    textColor: "{colors.fg}"
    rounded: "{rounded.pill}"
    height: "22px"
---

# Design System: nicecooling

The comps in `docs/ui/comps/` are the visual source. The icons are in `docs/ui/icons/`. `docs/ui/comps/nc.css` has every token as a CSS custom property, in light and dark. The frontmatter above has the light values. `docs/ui/screens/` has one document per screen. `docs/ui/widgets.md` has the custom widgets.

## Overview

nicecooling looks like a GNOME system tool. It sits next to GNOME Settings, Resources and Disks and has the same finish: neutral greys, one blue accent, rounded cards, a sidebar, a header bar. The owner chose this standard on 2026-11-22.

The app has one signature: the Loop diagram. Each Loop is one piping diagram, and each instrument bubble shows a live value. An Alarm sits on the part that causes it. The Leakshield Status schematic uses the same notation. No other part of the app adds decoration.

The app follows the system theme. The user works at a desk, by day and in the evening, so light and dark are equal.

Rules of the direction:

- **Standard controls.** Every control is a gpui-component primitive with these tokens. A custom element exists only where the table in "Component map" says so.
- **Color has jobs.** Blue is selection, the suggested action, links and the water path. Red, amber and grey are severity. A normal screen shows blue and neutrals only.
- **Severity is never color alone.** Each severity has a shape, a color and a text label.
- **Everything is buildable in GPUI.** No blur, no gradient, no filter. Shadows are plain box shadows. Dashed lines, hatch fills and strikethrough text are painted by the custom elements or by GPUI text styles: check each against the pinned `gpui` version.

## Colors

### Neutral and accent

| Token | Light | Dark | Use |
|---|---|---|---|
| `window` | `#FAFAFB` | `#222226` | Content background |
| `view` | `#FFFFFF` | `#1D1D20` | Plot background, text views |
| `sidebar` | `#EBEBED` | `#2E2E32` | Sidebar |
| `card` | `#FFFFFF` | `#36363A` | Cards, dialogs, popovers |
| `headerbar` | `#FFFFFF` | `#2E2E32` | Header bar, tab strip |
| `fg` | `#2A2A2E` | `#FFFFFF` | Text, part outlines |
| `fg-dim` | `#5E5E66` | `#B4B4BC` | Captions, units, axis labels |
| `border` | `#DCDCE0` | `#47474D` | Row and pane separators, grid lines |
| `border-strong` | `#B8B8C0` | `#6A6A72` | Axis lines, read-only field outline, switch off |
| `hover` | `#F0F0F2` | `#3E3E43` | Hover fill, inline editor ground |
| `selected` | `#D9D9DD` | `#4A4A50` | Selected sidebar row, default button, neutral tag |
| `track` | `#E4E4E8` | `#444449` | Input fill, slider and progress track |
| `accent-bg` | `#1C71D8` | `#1C71D8` | Suggested button, switch on, focus ring, selected tab line |
| `accent-fg` | `#FFFFFF` | `#FFFFFF` | Text on `accent-bg` |
| `accent` | `#0461BE` | `#78AEED` | Links, accent text and icons |
| `accent-tint` | `#E3EEFB` | `#263A55` | Selected row, info banner, accent tag |

### Severity

| Token | Light | Dark | Use |
|---|---|---|---|
| `alarm-bg` | `#C01C28` | `#C01C28` | Alarm banner, Alarm bubble, destructive button |
| `alarm-fg` | `#FFFFFF` | `#FFFFFF` | Text on `alarm-bg` |
| `alarm` | `#C01C28` | `#FF938C` | Alarm text and icons on a neutral ground |
| `alarm-tint` | `#FBE4E5` | `#4A2226` | Ground of a failed write row |
| `warning-bg` | `#E5A50A` | `#E5A50A` | Warning bubble |
| `warning-fg` | `#2A2A2E` | `#2A2A2E` | Text on `warning-bg` |
| `warning` | `#905400` | `#FFC252` | Warning text and icons on a neutral ground |
| `warning-tint` | `#FDF3D9` | `#3D3421` | Warning banner, warning row |
| `success` | `#1B8553` | `#78E9AB` | Verified badge, "In range" tag |
| `ok-dot` | `#2EC27E` | `#2EC27E` | Daemon connected dot |
| `offline` | `#6B6A70` | `#A3A3AB` | Offline text, icons and dashed outlines |
| `offline-tint` | `#EBEBED` | `#2E2E32` | Offline banner |
| `experimental` | `#8939A4` | `#DC8ADD` | Reserved: Experimental devices are not in version 1 |
| `experimental-tint` | `#F3E6F6` | `#3B2A40` | Reserved |

### Diagram and data series

| Token | Light | Dark | Use |
|---|---|---|---|
| `water` | `#1C71D8` | `#62A0EA` | Water path of a diagram, fill level |
| `air` | `#77767B` | `#9A9996` | Air path of a diagram |
| `part` | `#F6F5F4` | `#2A2A2E` | Fill of a part symbol |
| `s1` to `s6` | `#1C71D8` `#2190A4` `#9141AC` `#D56199` `#3A944A` `#6F8396` | `#78AEED` `#5BC8DC` `#C29EE6` `#F08DBB` `#7ED492` `#A7B4C2` | Chart series, in this order |

A chart never uses red or amber for a series. A chart with more than six series repeats the colors with a dashed line.

### Named rules

- **The Alarm field.** An active Alarm is a solid `alarm-bg` field with `alarm-fg` text: the banner, the bubble, the dialog head. "Daemon down" uses the same field, because the nicecooling protection is off. No other state has a solid field.
- **Tint for the rest.** Warning, offline and information banners use their tint as ground and normal `fg` text.
- **Contrast.** Body text and captions have at least 4.5:1 on their ground. Never put `fg-dim` on a tint: use `fg`.

## Typography

One family: Adwaita Sans, the GNOME interface font. Use tabular figures everywhere, so live values do not move. Adwaita Mono is only for instrument tags, serial numbers, event times and raw device values.

| Role | Size / line | Weight | Use |
|---|---|---|---|
| `caption` | 12 / 16 | 400 | Captions, units, axis labels, table headers (700) |
| `body` | 14 / 20 | 400 | Text, rows, controls |
| `heading` | 14 / 20 | 700 | Card titles, row titles, buttons, page title |
| `value` | 16 / 22 | 700 | Live value in a diagram bubble or a tile |
| `title` | 18 / 24 | 700 | Dialog title |
| `numeral` | 28 / 32 | 800 | Gauge value, banner countdown |
| `countdown` | 72 / 76 | 800 | Seconds in the poweroff countdown dialog |
| `tag-mono` | 12 / 16 | 700 | Two-letter instrument tags |

- A small button (28 px high) has 13 px text.
- Sentence case for every label. Mode names stay in capitals: SHIELD, MONITOR, RELEASE.
- A value and its unit are separate text runs: the unit is `fg-dim` and regular weight.
- A space groups the thousands of a number in a text, in English and in Italian: "2 840 rpm". The space does not break the line. A number in an input has no grouping.
- Text length: an Italian string is often 20 to 30% longer. A label never has a fixed width. A long name truncates with an ellipsis and the value next to it keeps its width.

## Layout

- Base unit 6 px. Spacing steps: 3, 6, 12, 18, 24, 36.
- Window: minimum 1100x700. Sidebar 220 px. Header bar 46 px. Status bar 28 px. Tab strip 38 px.
- Page: padding 12 px top, 24 px sides. Gap between cards 12 px. A page scrolls vertically. It never scrolls or crops horizontally.
- Card: header 40 px with 18 px side padding, rows 40 px (30 px in a tight list), body padding 18 px.
- Control height 34 px (28 px for a small button or an inline input). Table row 36 px, or 48 px with a caption line.
- **One layout.** The layout is the same at every window size from 1100x700 up. In a list and detail screen the list column has a fixed width (360 px in Channels, 292 px in Loops and Profiles) and the detail column takes the rest. A secondary card is under its main card, not beside it. A card header has one text button at most; other commands are in a menu button.
- **Tab overflow.** A tab strip that does not fit puts its last tabs in a "More" menu.

## Elevation & Depth

- Light theme: a card has the Adwaita card shadow: `0 0 0 1px rgba(0,0,6,.03), 0 1px 3px 1px rgba(0,0,6,.07), 0 2px 6px 2px rgba(0,0,6,.03)`.
- Dark theme: a card has no shadow. The lighter `card` fill separates it.
- Dialog and popover: `0 0 0 1px rgba(0,0,6,.06), 0 8px 24px rgba(0,0,6,.18)` in light, `0 0 0 1px rgba(255,255,255,.08), 0 8px 24px rgba(0,0,0,.5)` in dark. A dialog has a scrim: `rgba(0,0,6,.5)` in light, `rgba(0,0,0,.6)` in dark.
- No other depth. No blur.

## Shapes

- Radii: 6 px for small parts (tooltip, plot label), 9 px for controls and inputs (7 px for a small button and an inline input), 12 px for cards, 15 px for dialogs, full pill for tags and switches.
- Outlines: 1 px `border` for separators. 1.5 px `fg` for part symbols and bubbles. 3 px `water` for the water path. 2 px dashed `air` for the air path.
- Dashed means "not live" or "not confirmed": offline bubble, read-only field, the "Blocks and radiators" box.
- Focus ring: 2 px `accent-bg`, 2 px offset, on every focusable element.

## Motion

- State change (hover, selection, switch, tab): 150 ms, ease-out.
- Dialog and popover: 200 ms, ease-out, opacity and a 4 px rise.
- Live values change without animation. The Curve editor operating point moves in a straight line in 150 ms.
- An Alarm appears at once: no fade, no flash, no pulse. The countdown number changes once per second without animation.
- When the system asks for reduced motion, every duration is 0.

## Severity system

Each state has four signals: icon shape, color, text label, and a place in the shell. A user who cannot see color still reads the shape and the label.

| State | Icon | Color tokens | Label | Where it shows |
|---|---|---|---|---|
| Normal | none | `fg`, `ok-dot` for the daemon | value only | No banner. Status bar: "No active Alarms" |
| Warning | filled triangle with "!" | `warning`, `warning-bg` (bubble), `warning-tint` (banner) | "Warning" plus its type | Bubble of the part, row in Active Alarms, status bar count, tray icon |
| Alarm | filled octagon with "!" | `alarm-bg` + `alarm-fg` (solid), `alarm` (text) | "Alarm" plus its type | Red banner, bubble of the part, countdown dialog, sidebar entry of the device, status bar, tray icon |
| Offline | dashed circle with a slash | `offline`, `offline-tint` | "Daemon offline", "Leakshield unreachable" or "No signal" | Banner, dashed bubble with "--", every write control disabled |
| Daemon down | filled octagon with "!" | `alarm-bg` + `alarm-fg` (solid), `alarm` (text) | "Daemon down" and "nicecooling protection is off" | Red banner, entry in Active Alarms, status bar, tray icon in the Alarm state, every write control disabled |
| Read-only | padlock, dashed field outline | `fg-dim`, `border-strong` | "Read-only" plus the reason | Next to the field name; the reason is a caption under it |
| Experimental | flask, dashed pill | `experimental`, `experimental-tint` | "Experimental" | Reserved. Experimental devices are not in version 1 |

Rules:

- **Order.** When several banners apply, the order from the top is: poweroff countdown, daemon down or offline, unknown firmware, Immediate switching on. A screen shows at most three banners. Others collapse into "2 more notices" on the last one.
- **Verification state** of a Leakshield field is a badge, not a severity: see `widgets.md`, "Verification state badge".
- **Write feedback** uses the same icons: check (`success`) for "Verified", octagon (`alarm`) for "Failed".

## Components

### Buttons

- Default: `selected` fill, `fg` text, weight 700, 34 px, radius 9 px.
- Suggested: `accent-bg` fill, `accent-fg` text. One per view at most.
- Destructive: `alarm-bg` fill. Only for factory reset, delete and restore.
- Flat: no fill. For secondary commands in a row.
- Link: `accent` text, weight 700, no fill and no underline. It opens a page, a tab or a dialog. A card row that opens a page has the link at the left and a chevron at the right, and the whole row takes the click.
- On the Alarm banner: white fill, text `#A51D2D`.

### Tags

22 px pill, 12 px text, weight 600. Neutral (`selected`), accent (`accent-tint` + `accent`), warning (`warning-tint` + `warning`), Alarm (`alarm-bg` + `alarm-fg`), offline (dashed `offline` outline), success (1 px `success` outline). A Mode tag has letter spacing 0.04 em and weight 700.

### Cards and lists

A card has a header and rows. Rows have a 1 px `border` line above them. Do not put a card in a card. A group of settings is one card with one row per setting: the name and its caption at the left, the control at the right.

### Inputs

`track` fill, no outline, radius 9 px, the unit at the right in `fg-dim`. A read-only field has no fill and a dashed `border-strong` outline. Focus adds the focus ring.

### Navigation

Sidebar rows are 34 px with an icon and a label. The selected row has the `selected` fill and weight 600. Groups have a 12 px bold `fg-dim` label. The header bar has the page title in the center, the device serial as subtitle on a device page, the primary menu button and the close button at the right.

### Loop diagram (signature)

See `widgets.md`, "Loop diagram". 168 px high. Reservoir and Leakshield at the left. Pump, flow sensor and water temperature on the supply leg at 34%, 58% and 80% of the width. Each has an 18 px bubble above it with a two-letter tag: PT pressure, ST speed, FT flow, TT temperature, LT level.

## Icons

Sources and exports are in `icons/`. `icons/preview.html` shows them; `icons/preview-light.png` and `icons/preview-dark.png` are its renders.

### Interface icons

The icons of the sidebar, the severity states, the padlock and the chevrons are in `crates/nicecooling/assets/nc/`: one SVG per icon on a 16 px grid, stroke 1.5 px or filled. They have the shapes of the sprite in `comps/nc.js`. The app embeds them and draws each one in the color of its token.

### App icon

`icons/nicecooling.svg`, drawn on a 128 px grid. It is the Loop diagram as an object: a blue water loop, and on its top leg an instrument bubble that holds a drop.

- Colors, from the Adwaita palette: loop `#3584E4` with a `#1A5FB4` lower edge and a `#62A0EA` highlight, bubble ring `#1A5FB4` with a `#1A3F7A` lower edge, bubble face `#FFFFFF`, drop `#3584E4`.
- No text, no shield, no mark of another product.
- PNG exports: `icons/png/nicecooling-<size>.png` for 16, 24, 32, 48, 64, 128, 256 and 512 px.
- Install the SVG as the scalable app icon under the application id of the package. Below 24 px the desktop uses the symbolic icon.

### Symbolic and tray icons

Four SVG files on a 16 px grid, one color (`#2E3436`). The desktop recolors a symbolic icon for a light or a dark top bar, so the state is a shape.

| State | File | Shape |
|---|---|---|
| Normal, and the symbolic app icon | `icons/nicecooling-symbolic.svg` | A rounded loop with a drop in it |
| Warning | `icons/nicecooling-warning-symbolic.svg` | The loop with a triangle and "!" at the lower right. The triangle has the class `warning` |
| Alarm, and daemon down | `icons/nicecooling-alarm-symbolic.svg` | The loop with an octagon and "!" at the lower right. The octagon has the class `error` |
| Offline | `icons/nicecooling-offline-symbolic.svg` | The loop at 35% opacity with a slash |

- A badge has a 1 px clear ring around it, made with a clip path.
- GTK and GNOME Shell give the classes `warning` and `error` the warning and error colors of the theme. The shape carries the state where a tray does not do that.
- PNG exports for a tray that takes pixmaps: `icons/png/symbolic/<name>-<size>.png` in the dark ink and `<name>-light-<size>.png` in white, for 16, 24, 32 and 48 px.
- In the Alarm state the app also sets the StatusNotifierItem status to "NeedsAttention".

## Component map

"Primitive" names are the gpui-component modules. Check the exact type names against the pinned version of `gpui-component`. "Composite" means a small view made of primitives, with no custom painting. "Custom" means a custom GPUI element with its own paint code: its spec is in `widgets.md`.

### App shell, tray and notifications

| Component (`components.md`) | Built with |
|---|---|
| Window and header bar | Composite: a GPUI flex row with the page title, the primary menu `button` and one round close `button`. The row moves the window, and a double click maximizes or restores it. The window has client-side decorations where the desktop gives no title bar (GNOME on Wayland). Where the window manager draws its own title bar, the row has no close button |
| Sidebar navigation and groups | `sidebar` |
| Status bar | Composite: a GPUI flex row |
| Global banners | Composite `Banner`: icon, text, optional `button` |
| Write feedback | Composite `WriteStatus` (icon, text, `progress`) in the row, plus `notification` for the result |
| Tray icon and menu | StatusNotifierItem and its D-Bus menu. Not GPUI |
| Desktop notifications | `org.freedesktop.Notifications`. Not GPUI. In-window results use `notification` |

### Screens

| Component (`components.md`) | Built with |
|---|---|
| Overview: Loop card | `LoopDiagram` (custom) in a card |
| Overview: key temperatures, active Alarms, active Profile | Composite list rows, `tag` |
| Loops: list and editor | List rows, `input`, `select`, `SensorPicker` (custom) |
| Loops: Pump floor control | `slider` and number `input` |
| Loops and Sensors: Calibration table editor | `CalibrationEditor` (custom): `table` bound to `plot` |
| Loops: Feed status | Composite list rows |
| Channels: list | `table` |
| Channels: Curve editor | `CurveEditor` (custom), on the scales of `plot` |
| Channels: Input picker | `SensorPicker` (custom) in a `popover` |
| Channels: rename | `input` in a `popover` |
| Channels: BIOS snapshot panel | Collapsible row with a `table` |
| Sensors: grouped list with sparkline | `table` with group rows, line `chart` at row height |
| Sensors: rename, hide | `input` in a `popover`, `switch` |
| Sensors: Derived sensor editor | `dialog`, `select`, `SensorPicker` (custom) |
| Leakshield: tabs | `tab` with an overflow `menu` |
| Leakshield Overview: pressure gauge | `PressureGauge` (custom) |
| Leakshield Overview: status values | Composite list rows, `tag` |
| Leakshield Overview: Mode selector | Button group with three toggle `button`s |
| Leakshield Overview: Action bar | `button`s |
| Leakshield Overview: pressure chart, pump runtime chart | Area `chart`, bar `chart` |
| Action view: Test | Number `input`, line `chart` with zoom on `plot`, `select` for the source, `button`s |
| Action view: Measure fill level, Deaerate | `progress`, `Banner` |
| Action view: Refill | Composite instruction card |
| Leakshield Settings: wizard | `stepper`, settings form (`switch`, `select`, `input`), `button`s |
| Leakshield Settings: Summary | `DiffTable` (custom) and `WriteStatus` |
| Leakshield Sensors | `table` with inline `input`, `SensorPicker` (custom), `select`, number `input` |
| Leakshield Display: page grid and previews | `DisplayPreview` (custom) in a grid |
| Leakshield Display: template gallery | `dialog` with a grid of `DisplayPreview` |
| Leakshield Display: logo upload | File dialog of the system, `LogoPreview` (custom) |
| Leakshield Display: settings | Settings form: `slider`, `switch`, number `input` |
| Leakshield Lighting: LED timeline | `LedTimeline` (custom) |
| Leakshield Lighting: effect picker, parameters | `dialog`, color picker, `slider`, `SensorPicker` (custom) |
| Leakshield Alarms | `table` with `slider`, `switch` and `select` cells, `Banner` for overlap warnings |
| Leakshield Status: schematic | `DeviceSchematic` (custom), same painter as `LoopDiagram` |
| Leakshield Status: values, chart, events | Radio tiles, line `chart`, `table`, `button` |
| Leakshield System | Composite list rows, `table` of backups, `select`, `button` |
| Leakshield Fields | `table`, `VerificationBadge` (custom) |
| Profiles: list | List rows, `menu`, `dialog` for rename and delete |
| Profiles: content | Composite list rows, `Banner` row for differences |
| Profiles: rule list | Sortable list rows with a drag handle |
| Profiles: rule editor | `select`, number `input`, time `input`, `SensorPicker` (custom), `ProcessPicker` (custom) |
| Profiles: options | Number `input`, `switch`, warning `dialog` with a checkbox |
| Alarms: active and history | `table` |
| Alarms: alarm response table | `table` with `select` and number `input` cells |
| Alarms: poweroff countdown | `CountdownDialog` (custom), on `dialog` |
| History: chart | `HistoryChart` (custom), on `plot`: pan, zoom, gaps |
| History: sensor multi-select, range | `SensorPicker` (custom) with check marks, button group |
| Settings | Settings form |
| First run | `stepper`, composite cards, `LoopDiagram` (custom) as preview |

## Do's and Don'ts

### Do:

- **Do** use the glossary terms of `GLOSSARY.md` in every label, with their capitals: Loop, Channel, Duty, Curve, Input, Pump floor, Profile, Mode, Action, Warning, Alarm.
- **Do** keep "Action" for the Leakshield procedures: Test, Measure fill level, Deaerate, Refill. The reaction that the user sets for a Warning or an Alarm is the "alarm response": none, notify, or poweroff after a delay. The reaction of the Leakshield itself to its alarms is the "Device response": signal output, key sequence, sound.
- **Do** put the reason next to every disabled or read-only control.
- **Do** show a value with its unit and its limit: "298 mbar, target 300".
- **Do** draw in a diagram only what the Loop model holds. Parts that nicecooling does not measure are one dashed box.
- **Do** keep the Alarm banner and "Cancel poweroff" visible on every screen while a countdown runs.

### Don't:

- **Don't** use red or amber for anything that is not an Alarm or a Warning.
- **Don't** show a state with color alone.
- **Don't** add stat tiles, round gauges or sparklines as decoration. A chart is there when the user needs the history of a value.
- **Don't** copy the look, the logo or the names of the Aqua Computer software. nicecooling takes its workflows from the device manual, not its identity.
- **Don't** use a web-only effect in a comp: no blur, no gradient, no filter.
