# Root daemon with a GPUI client over D-Bus and polkit

nicecooling is a root system daemon, `nicecoolingd`, and a GPUI desktop client. Curves and the Feed must run when the window is closed, and the writes to hwmon and hidraw need root. The client talks to the daemon over the D-Bus system bus, and polkit authorizes each write action. We chose D-Bus and polkit over a group-owned Unix socket to get per-action authorization; the cost is more code and more system configuration.
