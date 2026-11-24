# Leakshield: Display tab

No comp. The widgets are in `../widgets.md`: "Display page preview, template schematic and import preview".

The Leakshield has a 128x64 OLED display with 10 pages. The tab shows what the device draws for each page, sets the content of each page and sets the display behavior.

## Layout

1. Card "Pages": a grid of 10 display page previews, 5 per row. Five previews of 136 px fit the 832 px of the minimum window. Each preview has its number and the template name under it. The selected page has the accent outline.
2. Card "Page 3": the selected page.
   - At the left: the large preview (128x64 at 2 px for each pixel), the reason when it has no image, the caption "The Leakshield shows the page for 30 s each time nicecooling reads its image." and the button "Refresh preview".
   - At the right: "Template" with the name and the button "Change template", then the switches "Show in standby" (bit 0 of `page_options`) and "Hide in normal operation" (bit 1).
   - Under both: the slots that the template uses, and no other slot. The section on the slots of a template has the table.
3. Card "Display settings": "Brightness" and "Brightness when inactive" (selects), "Automatic page change" (switch) and "Interval" (1 to 29 seconds), "Rotate 180 degrees" (switch), "Invert" (switch), "Automatic invert" (switch), "Off in standby" (switch).

## Previews

A preview is the image that the device draws, read with `GetDisplayPreview`. The app draws no page by itself.

- The app reads the image of a page when the user selects the page, when the tab opens and the selected page has no image yet, with "Refresh preview", and after each write that changes the page. The device then shows that page for 30 s.
- The app keeps the last image of each page while it runs. A page that was not selected yet has no image.
- A lit pixel is one square of 1 px in the grid and of 2 px on the page card, in the colors `display` and `display-ink`.
- A preview without an image is a light box with a dashed outline and a short text. The page card has the full reason next to its preview. A preview is never a dark box without a reason.

| State | Text in the preview | Reason on the page card |
|---|---|---|
| Not read yet | "Not read yet" | "No image yet. nicecooling reads it when you select the page." |
| A read runs | "Reading", or the last image | "nicecooling reads the page from the Leakshield." |
| No template | "Empty" | "This page has no template. The Leakshield skips it." |
| `PageNotShown` | "Not shown" | "The Leakshield does not show this page now: its display has another page, or none." |
| `ReadOnly`, or a read-only device without an image | "Read-only" | "The Leakshield is read-only. nicecooling sends it no command, so it cannot ask for the page." |
| `Disconnected`, or a Leakshield that does not answer, without an image | "No answer" | "The Leakshield does not answer." |
| Daemon offline, without an image | "Daemon offline" | "The daemon is offline." |
| The daemon lacks the method | "Not available" | "This daemon cannot read the display. Update nicecoolingd." |
| `NotSupported` | "Not supported" | "This firmware has no command that shows a page." |
| Another error | "Read failed" | "The read failed:" and the error |

"Refresh preview" is disabled, with the reason as its tooltip, while a read runs, for a page without a template, for a read-only device, and while the daemon or the Leakshield does not answer.

## Slots of a template

A page has 8 value slots, 8 text slots, 4 icon slots and 8 limits. Each template uses a part of them. The table follows the page layouts of the device software.

| Template | Values | Texts | Icons | Bars |
|---|---|---|---|---|
| "Logo" | 0 | 0 | 0 | 0, and the logo |
| "Chart" | 1 | 0 | 0 | 0 |
| "1 value" | 1 | 1 | 1 | 0 |
| "2 values" | 2 | 2 | 2 | 0 |
| "4 values" | 4 | 4 | 0 | 0 |
| "3 values, layout A" | 3 | 1 | 2 | 0 |
| "3 values, layout B" | 3 | 1 | 1 | 0 |
| "Time" | 0 | 0 | 0 | 0 |
| "2 bars" | 2 | 2 | 2 | 2 |
| "4 bars, layout A" | 4 | 0 | 2 | 4 |
| "4 bars, layout B" | 4 | 1 | 0 | 4 |
| "Value and bar" | 1 | 1 | 0 | 1 |
| "Value and time" | 1 | 1 | 1 | 0 |
| "Leakshield state" | 0 | 0 | 0 | 0 |

- A value is one row of a table with the columns "Value", "Data source" and "Decimals". A template with bars adds "Minimum" and "Maximum".
  - "Data source": the device sensors 0 to 5, the Software sensors (sensors 8 to 19) and "No source". The device keeps the sensor index times 4 in `page_sources`.
  - "Decimals": 0 to 3 (`page_precisions`).
  - "Minimum" and "Maximum": the limits of the bar (`page_values`, two for each value), whole numbers from -32000 to 32000. A number outside is cut to the limit.
- A text is one row: a select with the names of the device that are not empty, then "No text" (`page_names`), and the button "Own text".
- An icon is one row: the image of the icon, its number or "No icon" (`page_icons`), and the button "Change icon".
- The "Logo" template has one row: the logo of the device and the button "Change logo".
- A template without a slot has the line "This template has no settings."
- The unit of a value comes from its sensor.

## Dialogs

Each dialog is 440 px wide. Escape and "Cancel" close it without a change.

- Template gallery ("Templates"): the 14 templates and "No page", 3 per row. Each one has a schematic of its layout that the app draws, and its name. The template of the page has the accent outline. A click writes the template and closes the dialog. "No page" writes 255: the device skips the page.
- "Own text": an input and "Save". The text goes into a page text of the device (names 20 to 34 of report 8), and the slot then selects it. The dialog names the page text that takes it. The rule for the page text:
  1. the page text that the slot shows now, when no other text slot in use shows it;
  2. else the first page text that is empty and that no text slot in use shows;
  3. else the first page text that no text slot in use shows.
  A text slot is in use when the template of its page shows it. A sensor name and a lighting name are never changed.
- "Choose an icon": the 16 icons of the device at 2 px for each pixel, 6 per row. A click selects the icon. "No icon" clears the slot. "Import an icon" opens the import for the icon that the slot shows.
- Import ("Change the logo", "Import an icon"): "Choose a file" opens the file dialog of the system. The dialog then shows the result, the threshold slider, the switch "Swap lit and dark" and the button "Write the logo" or "Write the icon". The section on the import of a picture has the rules.

## Import of a picture

The logo is 128x64 pixels and an icon is 23x23 pixels, each pixel lit or dark. The same import serves both.

- The file is a PNG file. Another file is not a picture for the import: "nicecooling cannot read this file as a PNG image."
- A file of another size is scaled, not refused. It keeps its aspect ratio, fits the target and sits in the middle on a dark ground. A smaller result takes the mean of the pixels. A larger result keeps hard edges. The dialog says so: "The file has 640 x 480 pixels. nicecooling scales it to fit 128 x 64." The user sees the result before the write, so every picture is a valid start.
- A pixel is lit when its brightness is at or above the threshold. The slider starts at 50 % and the result follows it while it moves. A transparent pixel is dark.
- The dialog shows the result at 2 px for each pixel (logo) or 4 px (icon). The write sends exactly this result with `SetDisplayLogo` or `SetDisplayIcon`.
- An imported icon replaces the icon that the slot shows, on every page that uses it. A slot with "No icon" has no icon to replace: "Pick an icon first. The import replaces the picked icon on every page that uses it."

## States

| State | What the user sees |
|---|---|
| Empty | A page without a template: the preview says "Empty" and the button is "Choose a template" |
| Loading | Grey bars |
| Device offline | Offline banner. The previews show their last images. Every control that writes is disabled |
| Daemon offline | The banner of the shell. Every control that writes is disabled, with "The daemon is offline." as the reason of the buttons |
| Read-only | With an unknown firmware a notice has the reason, and every control is disabled |
| Field not verified | The controls of a page field that is not a Verified field are disabled, with the reason of the field as the tooltip of its button |
| Page off | A page that is hidden in normal operation and not shown in standby has a preview at 40% opacity and the caption "Not shown" |
| Value without source | A Warning row under the value: "Software sensor 3 has no data source." with the link "Open Sensors" |
| No names and icons | The daemon gave no assets: the text select has the 40 names by number, and "Own text" is disabled with "nicecooling has no texts and no icons of this Leakshield." |
| All page texts in use | "Own text" is disabled: "All 15 page texts are in use. Pick one of the texts of the list." |
| Text too long | "Save" is disabled: "The text has more than 23 characters." |
| Alarm | Not used. The device shows its own alarm page |

## Interactions

- "Automatic page change" is on while `display_page_time` is below 30 s. The switch off writes 30 s. The switch on writes the interval from before, or 29 s. "Interval" takes 1 to 29 s and is disabled while the switch is off. "Rotate 180 degrees" is bit 0 of `display_flags`.
- "Invert" is disabled while "Automatic invert" is on (bit 3 of `display_flags` clear). Its caption then reads "Turn automatic invert off to set it by hand."
- "Off in standby" is on while bit 1 of `display_flags` is clear. The display then goes off after 10 minutes in standby.
- A click on a preview selects the page and reads its image. Arrow keys move in the grid. Enter opens the template gallery.
- Each change of a page is one Leakshield write. Its write status is the last row of the page card. The app then reads the image of the page again.
- A minimum or a maximum writes when its input loses the focus or on Enter.
- The app warns and does not refuse a choice. A control whose write is not possible is disabled and has the reason.

## Copy

| Place | Text |
|---|---|
| Automatic invert | "The display inverts half of the time. This slows the wear of the OLED pixels." |
| Off in standby | "The display is off while the PC is in standby. This extends its life." |
| Software sensor hint | "A page shows values of the Leakshield and of Software sensors. For another value, assign it to a Software sensor first." |
| Logo row | "The Leakshield shows the logo when it starts, and on a page with the Logo template." |
| Own text | "The text goes into page text 4 of the Leakshield." |
| Threshold | "Lit from 50 % brightness" |
