# Proposal

## Why

A Leakshield firmware update now needs aquasuite in a Windows VM, with USB passthrough that loses the device when it re-enumerates. The owner wants nicecooling to update the firmware on Linux, so that the VM is no longer needed. The aquasuite GUI sends the encrypted image unchanged, so nicecooling does not need the key.

## What Changes

- The daemon reads a firmware image from the aquasuite package database (`Content/aquasuite_db.database`, table `firmware`). The user picks the file in the app, and the app passes the open file to the daemon. The daemon reads it read-only into memory and lists the Leakshield rows, each with a status from the checks. The user picks the English or the German image. nicecooling ships no firmware.
- The daemon flashes the image through the bootloader of the device. Before the flash it saves a backup. After the flash it reads the settings back. A device that it finds in the bootloader can resume the flash.
- The daemon accepts an image with the hardware and the bootloader of the device and a version or a display language that differs from the installed one. A device in the bootloader whose identity is unknown needs a second confirmation. A downgrade needs an extra confirmation. After the flash the settings stay read-only until the field registry covers the new version.
- Two new D-Bus methods list the images (polkit action `write`) and start the update (polkit action `critical`). Signals report the progress.
- The Leakshield system screen of the app gets an update panel: image selection, the checks, the warnings, the progress and the result.
- ADR 0004 changes: the firmware update is in scope.

## Capabilities

### New Capabilities

- `leakshield-firmware-update`: image source and selection, the accepted versions, the flash through the bootloader, the recovery from the bootloader state, the API and the app panel.

### Modified Capabilities

None. The capabilities of `add-nicecooling-v1` are not archived yet. This change depends on that change and reads its contracts.

## Impact

- Code: `nicecooling-protocol` (image container and the bootloader protocol), `nicecooling-core` (the Alarm rules during an update), `nicecoolingd` (a firmware module, the device lease and the D-Bus methods), `nicecooling-api` (the contract), `nicecooling` (the update panel).
- API: two methods, two signals, a property and an error on the `Leakshield` interface of `io.github.cappyt.NiceCooling1`.
- Reverse engineering: `docs/protocol/leakshield-firmware-update.md` documents the bootloader protocol of aquasuite X.96.
- Hardware: the first flash is a supervised check with the owner. While the device is in the bootloader, the leak protection is off.
