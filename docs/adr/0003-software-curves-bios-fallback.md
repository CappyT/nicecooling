# Software curves, with the BIOS snapshot as fallback

The daemon runs every Curve in software and sets each Channel to manual Duty, because a Hardware curve can only read the sensors of the Super I/O chip, not the Leakshield, the GPU or a Derived sensor. Before it takes control, the daemon saves the BIOS snapshot. When it stops, and through `ExecStopPost` also after a crash, it writes the BIOS snapshot back. We rejected a fixed safe curve as fallback: it is louder and it ignores the setup the user made in the BIOS.

## Consequences

- If the kernel hangs, no code runs, and each Channel stays at its last Duty.
