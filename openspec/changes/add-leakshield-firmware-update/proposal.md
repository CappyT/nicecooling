# Proposal

## Why

A Leakshield firmware update now needs aquasuite in a Windows VM, with USB passthrough that loses the device when it re-enumerates. The owner wants nicecooling to update the firmware on Linux, so that the VM is no longer needed. The aquasuite GUI sends the encrypted image unchanged and the bootloader decrypts it, so nicecooling does not need the key.

## What Changes

- The daemon reads a firmware image from the aquasuite package database that the user points to (`Content/aquasuite_db.database`, table `firmware`). It opens the file read-only and selects the row that matches the hardware and the bootloader of the device. The user picks the English or the German image. nicecooling ships no firmware.
- The daemon flashes the image through the bootloader of the device. Before the flash it saves a backup. After the flash it reads the settings back. A device that it finds in the bootloader can resume the flash.
- The daemon accepts an image with the hardware and the bootloader of the device and a version that differs from the installed one. A downgrade needs an extra confirmation. After the flash the settings stay read-only until the field registry covers the new version.
- A new D-Bus method starts the update, with the polkit action `critical`, and signals report the progress.
- The Leakshield system screen of the app gets an update panel: image selection, the checks, the warnings, the progress and the result.
- ADR 0004 changes: the firmware update is in scope.

## Capabilities

### New Capabilities

- `leakshield-firmware-update`: image source and selection, the accepted versions, the flash through the bootloader, the recovery from the bootloader state, the API and the app panel.

### Modified Capabilities

None. The capabilities of `add-nicecooling-v1` are not archived yet. This change depends on that change and reads its contracts.

## Impact

- Code: `nicecooling-protocol` (image container and the bootloader protocol), `nicecoolingd` (a firmware module and its D-Bus method), `nicecooling` (the update panel).
- API: a new method and new signals on the `Leakshield` interface of `io.github.cappyt.NiceCooling1`.
- Reverse engineering: `docs/re/firmware-update.md` documents the bootloader protocol of aquasuite X.96.
- Hardware: the first flash is a supervised check with the owner. While the device is in the bootloader, the leak protection is off.
