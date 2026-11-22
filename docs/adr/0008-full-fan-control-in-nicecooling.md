# nicecooling keeps the full fan-control stack

nicecooling runs its own fan-control stack: Curves, Derived sensors, Input smoothing, the Pump floor, Alarms and the BIOS snapshot. One daemon couples the Pump floor, the Feed message, the pump stop, the Leakshield Inputs and the Profiles, and one app shows them. Code may be taken from CoolerControl where its license allows.

## Considered options

- A Leakshield-only daemon next to CoolerControl: rejected, because two daemons would write the Channels and the Leakshield without one safety model. The Pump floor, the pump stop and the Feed message need the same Channel state and the same flow value.

## Consequences

- nicecooling maintains the fan-control code itself.
- The license of CoolerControl is GPL-3.0. Its compatibility with nicecooling is an open item before any distribution.
