# Safety rules between Profiles, Actions, the pump and alarm actions

Profiles, Actions, the Pump floor and alarm actions each make sense alone but conflict on a real loop. We fix four rules:

- **Maintenance pause.** While an Action runs, or while the Mode is RELEASE set outside a Profile, Profile rules do not write the Leakshield; Curves still follow the rules. Otherwise a time rule can write SHIELD and start the vacuum on an open loop.
- **Refill only in standby.** The Leakshield manual requires the pump off during Refill, but the Pump floor and the pump-stop Alarm keep the pump on. nicecooling does not start Refill while the pump runs, and tells the user to start it with the device button with the PC in standby.
- **The daemon handles leaks.** The daemon's alarm responses, with their cancellable delay, handle leak Alarms. The signal output Device response stays as a backup only for the system Alarm, as a long press, for the case where Linux is hung. The key-sequence Device response stays off, because the Leakshield types it into the focused window, a terminal included. nicecooling proposes these device settings and warns when they overlap; it does not write them on its own.
- **No extra check on Profiles with critical values.** Saving a Profile that contains Mode RELEASE needs no admin password, although setting RELEASE directly does. The user accepted this risk.
