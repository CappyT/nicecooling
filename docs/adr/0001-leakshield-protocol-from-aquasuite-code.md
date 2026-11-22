# Learn the Leakshield protocol from aquasuite's code, verify on the reference device, write only verified fields

The Leakshield settings protocol has no public documentation, and the kernel driver `aquacomputer_d5next` covers sensors only. The device logic of aquasuite lives in its .NET service. We read the code of that service and rebuild the settings layout from it. The notes and the tools of that work are not in this repository. A field becomes a Verified field when aquasuite's code defines it and the registry marks it as verified for the device model and firmware version. Verification is a development-time procedure on the reference Leakshield. A low-risk field (display, lighting, names, Software sensors) is verified by a plausible read on the fixtures: the value lies in aquasuite's range and agrees with its default. A Safety field is verified only by a supervised Verification write in the manual hardware checks. A plausible read cannot prove the encoding of a value that controls the protection. The result is static registry data per model and firmware version. The app has no Verification write and no runtime verification ledger. nicecooling writes only Verified fields and keeps every other byte unchanged, because a wrong write can disable the leak protection of the loop. We avoid Windows: the `win11` VM is a last resort, used once to download aquasuite if Wine fails. The Leakshield firmware, which ships inside aquasuite, is a second source for ambiguous fields only.

## Considered options

- USB captures of aquasuite in the `win11` VM: rejected, because it makes Windows part of every protocol change.
- Confirm every field in the firmware too: rejected for the cost of MCU analysis on each field.
- Reuse a known layout for an unknown firmware version when sizes and values match: rejected, because it is speculative and a wrong layout writes a Safety field.

## Consequences

- A setting stays read-only when the code does not define it, the registry does not mark it as verified, or a device read contradicts it.
- An unknown firmware version is read-only. The firmware version is in report 1, bytes 13-14.
- Every write saves a backup of the device settings first, then reads them back and compares.
