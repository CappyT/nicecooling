# The desktop app is the session agent: notifications, tray icon and sensor history

The GPUI app starts at login without a window and shows a StatusNotifier tray icon. It is the only process that shows Alarm notifications, and it records the sensor history in SQLite in the user's data directory: 1 s samples for 48 hours, then 1 minute aggregates for one year. The root daemon keeps only the short windows that Curves and Profile rules need, and it writes nothing about history to disk. We chose this over a history database in `/var/lib` written by the daemon, to keep the root process small and the user data in the user's home.

## Consequences

- The history has gaps while no user session runs the app.
- On GNOME the tray icon needs the AppIndicator extension.
