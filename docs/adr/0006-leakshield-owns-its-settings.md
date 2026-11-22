# The Leakshield owns its settings

nicecooling reads the Leakshield settings from the device and writes them only when the user changes a setting or activates a Profile. It does not enforce its stored state on the Leakshield at boot or on drift, unlike the Board Curves, which the daemon applies all the time. The Leakshield is a safety device, and aquasuite or the device button can also change its settings, so a config-as-code loop would add writes and fight other tools.

## Consequences

- When another tool changes a Leakshield setting, nicecooling shows the new value, keeps it and sends a notification. The change does not enter any Profile: the next activation of a Profile writes the Profile's snapshot over it, and the notification says so.
- Changes made in nicecooling go into the active Profile.
