# Spike: windowless GPUI process with a tray icon and notification actions

Question: can the app (`nicecooling --background`, design D1) run as a GPUI process with no window, show a StatusNotifierItem tray icon (`ksni`) and send notifications with actions (`notify-rust`)? What does it cost at idle?

Code: `tools/spikes/gpui-tray` (outside the workspace, own `Cargo.lock`). Manual check: `docs/hardware-checks/gpui-tray.md`.

## Result

The spike builds and runs on the reference desktop. The checks below were run from the process side (verified from my side). The visual checks (icon in the top bar, no window, a real click on a notification button) are the owner's MANUAL checks and are not done.

| Check | Result |
|---|---|
| `gpui-pre =0.3.12` and `gpui-component 0.10.0` build | OK, with the three notes in the section "Build notes" |
| GPUI starts with no window | OK: `cx.windows()` is empty, `gpui_component::init` runs |
| Tray icon registers | OK: the AppIndicator watcher `org.kde.StatusNotifierWatcher` lists `org.kde.StatusNotifierItem-<pid>-1` |
| `GetCapabilities` of the notification server | OK: `actions`, `body`, `body-markup`, `icon-static`, `persistence`, `sound` |
| Tray menu callback reaches the process | OK, sent as a `com.canonical.dbusmenu` `Event` call from a shell: "Show notification" showed a notification, "Quit" ended the process |
| Notification action callback reaches the process and the GPUI foreground | OK, with a simulated `ActionInvoked` signal from `gdbus emit`. A real click is not tested |
| Shell lists a window of the process | Not verified. The `GetWindows` call of `org.gnome.Shell.Introspect` gave no match, but its access rule was not checked |

## Idle measurement

Setup: release build, rustc 1.100.0, GNOME Shell 51.1 on Wayland, extension `appindicatorsupport@rgcjonas.gmail.com` active. The process ran 11 s before the sample. The sample is 60 s with no menu use and no notification. CPU is the `utime + stime` delta of `/proc/<pid>/stat`. RAM is `VmRSS` in `/proc/<pid>/status`, read each second.

| Metric | Value |
|---|---|
| CPU | 0 ticks in 60 s (tick = 10 ms), so under 0.017 % of one core |
| RAM, `VmRSS` | 31492 KiB (30.8 MiB), the same in all 60 samples |
| RAM, `VmHWM` | 31492 KiB |
| Threads | 74 at start, 74 at the end |
| Binary | 53.9 MB, release, not stripped |

## Build notes

- `gpui-pre` has no entry point to make an `Application`. The function `gpui_platform::application()` is in the crate `gpui-pre-platform`. The spike adds `gpui-pre-platform = { version = "=0.3.12", features = ["wayland", "x11"] }`. The design table "Dependencies" does not list it. The app crate needs it.
- `ksni` 0.3.7 needs the feature `blocking` for `ksni::blocking::TrayMethods`. GPUI has no Tokio runtime. The default `ksni` API is async and needs one. The app crate sets `ksni = { version = "0.3.7", features = ["blocking"] }`, or runs the tray on its own Tokio thread.
- The release link needs `libxkbcommon-x11.so`. This Fedora has only the runtime file `libxkbcommon-x11.so.0`, because the package `libxkbcommon-x11-devel` is not installed. `cargo check` and the debug test builds do not link it. The spike was linked with `LIBRARY_PATH` that points to a directory with a symlink `libxkbcommon-x11.so` to the runtime file. The CI package `libxkbcommon-x11-dev` provides the file on `ubuntu-24.04`.
- `Application::run` blocks on the GPUI run loop. Tray and notification threads send events to the foreground through an `async-channel`, and the foreground task reads them with no polling.

## Open items

- The owner runs the procedure in `docs/hardware-checks/gpui-tray.md`.
- The spike did not test a tray icon on a desktop without the AppIndicator extension, a notification server that lacks `actions`, or an X11 session.
