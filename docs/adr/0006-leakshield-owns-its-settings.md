# The Leakshield owns its settings

nicecooling reads the Leakshield settings from the device and writes them only when the user changes a setting or activates a Profile. It does not enforce its stored state on the Leakshield at boot or on drift, unlike the Board Curves, which the daemon applies all the time. The Leakshield is a safety device, and aquasuite or the device button can also change its settings, so a config-as-code loop would add writes and fight other tools.

## Consequences

- When another tool changes a Leakshield setting, nicecooling shows the new value, keeps it and sends a notification. The change does not enter any Profile. A Profile activation writes the stored report 3 of the Profile's Leakshield slot only when the slot number differs from the device profile number of the device, as aquasuite does. Thus the activation overwrites an external change only when the numbers differ, and it keeps the change when they are equal. The notification says so.
- Curve changes made in nicecooling go into the active Profile. Leakshield setting changes go into the slot of the active Profile.
