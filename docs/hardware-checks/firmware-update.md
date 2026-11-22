# Leakshield firmware update in a VM

This procedure updates the Leakshield firmware with aquasuite in a Windows VM. nicecooling does not update firmware (ADR-0004). Each step has a pass line.

## Preconditions

- A libvirt VM with a virtual GPU (virtio, SPICE), TPM 2.0, Secure Boot and the NAT network. Reference: `win11-aquasuite`, Windows 11 IoT Enterprise 26H2, aquasuite X.96.
- The loop is closed and no maintenance runs. The leak protection is off while the device is in the VM or in its bootloader.

## Steps

1. Give the device to the VM with a libvirt `hostdev` that selects it by vendor `0x0c70` and product `0xf014`.
2. Start a watcher on the host before the flash. The device re-enumerates when it enters the bootloader and when it leaves it, and a `hostdev` keeps the old device number. The watcher reads `journalctl -kf` and, for each `usb <port>: New USB device found` line of the Leakshield port, detaches `hostdev0` and attaches the device again by vendor and product.
3. Do not use SPICE USB redirection with an auto-redirect filter. On the reference desktop it lost the device when it entered the bootloader, and the update stopped there.
4. In the VM, start aquasuite and run the firmware update.
5. Shut down the VM. Stop the watcher.
6. On the host, read report 1 and check bytes 13-14.

## Bootloader state

When an update stops in the bootloader, the device shows `bcdDevice` 2.00, two USB interfaces (no keyboard interface), a short report 1 with byte 2 = 0x01 and bytes 13-14 = 0x0001. The kernel hwmon device `leakshield` reads 0 for all sensors. Give the device to the VM again with step 1 and step 2, and run the update again.

## Record: 2026-11-23, serial 20417-31072

| Step | Result |
|---|---|
| 1 | pass |
| 2 | fail on the first run: no watcher, the device was lost on re-enumeration. pass on the second run: the watcher re-attached the device at 00:42:27 |
| 3 | fail on the first run: SPICE auto-redirect lost the device; the update stopped in the bootloader (state as above). Not used on the second run |
| 4 | pass on the second run |
| 5 | pass |
| 6 | pass: `bcdDevice` 1.00, three interfaces, report 1 bytes 13-14 = 0x03 0xFE (1022) |
