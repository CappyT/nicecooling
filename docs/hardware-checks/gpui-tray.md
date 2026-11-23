# GPUI tray and notification check

This procedure checks the spike `tools/spikes/gpui-tray` on GNOME with the AppIndicator extension. The spike is a GPUI process with no window, a StatusNotifierItem tray icon and a notification with actions. The result of the spike is in `docs/spikes/gpui-tray.md`. Each step has a pass line.

## Preconditions

- A GNOME Wayland session with the extension `appindicatorsupport@rgcjonas.gmail.com` enabled: `gnome-extensions list --enabled | grep appindicator`.
- A terminal in the session. The spike prints its events on stdout.

## Steps

1. Build the spike: `cargo build --release --manifest-path tools/spikes/gpui-tray/Cargo.toml`.
2. Start `tools/spikes/gpui-tray/target/release/gpui-tray`. It prints `gpui started, windows: 0`, `capabilities: [...]` and `tray registered`.
3. Check that the process has no window. The Overview and `Alt+Tab` show no entry for it, and the top bar shows the tray icon.
4. Check the notification server: `busctl --user call org.freedesktop.Notifications /org/freedesktop/Notifications org.freedesktop.Notifications GetCapabilities`. The list holds `actions`.
5. Open the tray icon menu and click "Show notification". The process prints `notification shown` and the shell shows the notification.
6. Click the button "Acknowledge" of the notification. The process prints `action invoked: ack` and `gpui foreground received action: ack`. When the notification shows no button, expand it first.
7. Click "Show notification" again and click the body of the notification. The process prints `action invoked: default`.
8. Click "Quit" in the tray menu. The process prints `quit`, exits with status 0, and the tray icon disappears.

## Record

2026-11-23, GNOME Shell 51.1, extension `appindicatorsupport@rgcjonas.gmail.com` enabled. The owner ran steps 3, 5, 6, 7 and 8; the log of the spike gives the rest.

| Step | Result |
|---|---|
| 1 | pass |
| 2 | pass: `gpui started, windows: 0`, `capabilities: [...]`, `tray registered` |
| 3 | pass (owner): tray icon visible, no window in the Overview or `Alt+Tab` |
| 4 | pass: the capabilities hold `actions` |
| 5 | pass: `notification shown, id 3` |
| 6 | pass: `action invoked: ack`, `gpui foreground received action: ack` |
| 7 | not observed: the log has no second `notification shown` and no `action invoked: default`. The app does not need a body click: the cancel path uses a button, the tray menu and the window |
| 8 | pass: `quit`, the process exited and the icon disappeared |
