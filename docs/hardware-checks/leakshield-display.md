# Leakshield display check

This procedure checks the display functions of the daemon on the reference Leakshield (serial 20417-31072, firmware 1022): the page preview (`show_page` and report 5) and the writes of report 8 (names, icons, logo). It answers the questions that `docs/protocol/leakshield.md` (sections Report 5 and Report 8) lists as not determined. The daemon writes of report 8 stay untrusted until this check passes. A person sits at the PC and looks at the display.

## Safety

- The check writes report 8 only. It writes no field of report 3. Take a backup first (step 1). Recovery restores it.
- A report 8 write rewrites the whole report. A failed write can leave names, icons or the logo wrong, never the pressure settings.
- The pump runs as usual. The check does not change the Mode.

## Steps

Variables as in `docs/hardware-checks/leakshield.md`: `B`, `R`, `LSID`, `LS`, `BC`.

1. `A=$($BC call $B $LS $B.Leakshield CreateBackup | cut -d'"' -f2); echo $A`. Expected: a backup id. The directory holds `report3.bin`, `report5.bin` and `report8.bin`.
2. Preview of each page: `for p in 0 1 2 3 4 5 6 7 8 9; do $BC call $B $LS $B.Leakshield GetDisplayPreview y $p | cut -c1-60; done`. Look at the display during the call. Expected: the display shows each page in turn, and each call returns `128 64` and 1024 bytes. Record which pages give `PageNotShown` (a page with template 255, or with bit 1 of `page_options` set).
3. Compare the image with the display: draw the bytes as a bitmap (any viewer) and look at the display. Expected: the image shows what the display shows. Record whether the image is inverted.
4. Wait 40 s. Expected: the display goes back to the normal page change. `show_page` leaves no trace.
5. Name: `$BC call $B $LS $B.Leakshield SetDisplayName y 20 s "Check"`. Expected: the call returns after about 1 s with no error. `GetDisplayAssets` shows `Check` at index 20. A page that uses text 21 (index 20 of `page_names`) shows it on the display. Record the time of the call.
6. Wait 6 s, then power-cycle the Leakshield (unplug and plug the USB cable and the power). Expected: after the start, `GetDisplayAssets` still shows `Check`. When it shows `Name 1` again, the device needs `save_settings` after report 8 and the daemon did not send it in time: record this.
7. Icon: write an icon with a diagonal line from the app or with `SetDisplayIcon`. Expected: a page that uses the icon shows the diagonal line in the right direction. Record the direction (column layout: bit 0 is the top line).
8. Logo: write the logo of the backup again, then a test pattern, with `SetDisplayLogo`. Expected: after the restart of the device the start screen shows the pattern. The call needs about 1 s.
9. Restore backup `A`: `$BC call $B $LS $B.Leakshield RestoreBackup s $A`. `RestoreBackup` writes report 3, then report 8 (the whole report, 250 ms, read-back). Expected: the call returns with no error and the display shows the old name, icons and logo again. Report 8 equals `report8.bin` of the backup; `GetDisplayAssets` equals the assets of the backup.

## Recovery

- Report 8 is wrong: run `RestoreBackup` with backup `A`. It writes report 8 of the backup when the device holds another one.
- The Leakshield does not answer: unplug and plug the USB cable.

## Record

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
