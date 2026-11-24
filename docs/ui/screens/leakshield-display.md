# Leakshield: Display tab

No comp. The widgets are in `../widgets.md`: "Display page preview and logo conversion preview".

The Leakshield has a 128x64 OLED display with 10 pages. The tab sets the content of each page and the display behavior.

## Layout

1. Card "Pages": a grid of 10 display page previews, 5 per row. Five previews of 136 px fit the 832 px of the minimum window. Each preview has its number and the template name under it. The selected page has the accent outline.
2. Card "Page 3": the settings of the selected page.
   - A large preview at the left (256x128). A click opens the template gallery.
   - "Template": the name and the button "Change template".
   - One row per value of the template: a picker limited to device sensors 0 to 5 and Software sensors 8 to 19 (the device keeps the sensor index times 4 in `page_sources`), and an icon select where the template has an icon.
   - With the "Logo" template: "Choose an image" and the logo conversion preview.
   - "Show in normal operation": switch. "Show in standby": switch.
3. Card "Display settings": "Brightness" (slider, percent), "Brightness when inactive" (slider), "Display off when inactive" (switch), "Automatic page change" (switch) and "Interval" (seconds), "Rotate 180 degrees" (switch), "Invert" (switch), "Automatic invert" (switch), "Off in standby" (switch).

## Template gallery

A dialog, 640 px wide: a grid of previews with the template names, 4 per row. A click selects a template and closes the dialog. Escape closes it without a change.

## States

| State | What the user sees |
|---|---|
| Empty | A page without a template shows an empty preview with "Empty" and the button "Choose a template" |
| Loading | Grey previews |
| Device offline | Offline banner. Previews show the last settings. Controls are disabled |
| Read-only | With an unknown firmware every control is read-only (banner 3). Display fields are low-risk fields: they are Verified fields after a plausible read |
| Page off | A page with both switches off has a preview at 40% opacity and the caption "Not shown" |
| Value without source | The preview shows "--". Warning row: "Software sensor 3 has no data source." with the link "Open Sensors" |
| Logo too large or wrong type | "Use a PNG, JPEG or BMP image." The conversion preview stays empty |
| Uploading the logo | Write status with a progress bar: "Writing the logo, 60 %" |
| Alarm | Not used. The device shows its own alarm page |

## Interactions

- A click on a preview selects the page. Arrow keys move in the grid. Enter opens the template gallery.
- Each change is a Leakshield write with write status in its row. Sliders write when the drag ends.
- The preview renders the template with live values, so the user sees the result before the device does.
- The logo conversion preview has a threshold slider and "Invert". "Write the logo" starts the upload.

## Copy

| Place | Text |
|---|---|
| Automatic invert | "The display inverts half of the time. This slows the wear of the OLED pixels." |
| Off in standby | "The display is off while the PC is in standby. This extends its life." |
| Software sensor hint | "A page shows values of the Leakshield and of Software sensors. For another value, assign it to a Software sensor first." |
