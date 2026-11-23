# Leakshield firmware update capture

This procedure watches two aquasuite firmware updates of the Leakshield in the Windows VM and records the USB traffic on the host with `usbmon`. The capture only observes. No tool of this procedure sends anything to the device. It settles the open points of `docs/re/firmware-update.md` (Open points and checks). Each step has a pass line.

The update flashes the device. The leak protection is off while the device is in the VM or in its bootloader. The loop must be closed and no maintenance may run during both runs.

## Preconditions

- The VM `win11-aquasuite` of `docs/hardware-checks/firmware-update.md`, with aquasuite X.96. The `hostdev` and the re-attach watcher of that procedure work.
- The reference Leakshield runs firmware 1022 with the English display language.
- The loop is closed and no maintenance runs. The leak protection is off during the update.
- `wireshark-cli` is installed (`dumpcap` and `tshark`), and `sudo` works.
- The unit `nicecoolingd` is stopped. The Alarm `leakshield-unreachable` can power off the PC while the device is in the VM.
- `re/captures/` exists. Git ignores `re/`.

## Runs

- Run 1: aquasuite in German and the device in English. aquasuite flashes the DE image (a language change). The device ends in German.
- Run 2: aquasuite in English and the device in German. aquasuite flashes the EN image. The device ends in English again.

Report 1 bytes are counted from the report ID at byte 0. Bytes 13-14 are the firmware version (`03 fe` is 1022). Byte 34 is the display language (`00` English, `01` German).

## Steps

### Preparation

1. Load the monitor: `sudo modprobe usbmon`. Expected: `sudo dumpcap -D` lists the interfaces `usbmon1` and up.
2. Stop the daemon: `sudo systemctl stop nicecoolingd`. Expected: `systemctl is-active nicecoolingd` prints `inactive`.
3. Find the bus of the device: `lsusb -d 0c70:f014`. Write the bus number `<N>` in the record. Expected: one line, `Bus 00<N> Device <D>: ID 0c70:f014`.

### Run 1: aquasuite in German, device in English

4. Read report 1 of the device on the host. Find its hidraw node `/dev/hidrawM` with step 1 of `docs/hardware-checks/fixture-dump.md`, then run `sudo dd if=/dev/hidrawM bs=837 count=1 status=none | xxd -p -c 837 | cut -c 27-30,69-70`. The command reads one input report. Expected: it prints `03fe00` (firmware 1022, English).
5. Give the device to the VM with step 1 of `docs/hardware-checks/firmware-update.md`, and start the VM. Expected: in the VM, aquasuite shows the Leakshield.
6. In aquasuite, set the user interface language to German and restart aquasuite. Expected: aquasuite shows German texts and the System page of the Leakshield shows firmware 1022.
7. Start the capture on the host: `sudo dumpcap -i usbmon<N> -w re/captures/firmware-update-run1-de.pcapng`. Expected: dumpcap prints `Capturing on 'usbmon<N>'`, and the file exists and grows.
8. Start the re-attach watcher of step 2 of `docs/hardware-checks/firmware-update.md`. Expected: the watcher runs and prints no error.
9. In aquasuite, press the update button of the System page and wait. Do not touch the VM until aquasuite ends. Expected: aquasuite reports the update as finished without an error, and the watcher logs two re-attaches (the device enters the bootloader, then leaves it).
10. Stop the watcher and the capture (Ctrl-C), shut down the VM, then run `sudo chown $USER re/captures/firmware-update-run1-de.pcapng`. Expected: the capture file is larger than 150 KB.
11. Read report 1 again as in step 4. Expected: it prints `03fe01` (firmware 1022, German).
12. Summarize the capture: `tshark -r re/captures/firmware-update-run1-de.pcapng -T json > re/captures/firmware-update-run1-de.json`, then `python3 -I tools/re/usbmon-summary.py --bus <N> re/captures/firmware-update-run1-de.json`. Expected: the summary prints the length of feature report 32, 108 SET_REPORT lines for report 32, and a time for each of the two re-enumerations.

### Run 2: aquasuite in English, device in German

13. Read report 1 as in step 4. Expected: it prints `03fe01`.
14. Give the device to the VM as in step 5, and start the VM. Expected: in the VM, aquasuite shows the Leakshield.
15. In aquasuite, set the user interface language to English and restart aquasuite. Expected: aquasuite shows English texts and the System page of the Leakshield shows firmware 1022.
16. Start the capture: `sudo dumpcap -i usbmon<N> -w re/captures/firmware-update-run2-en.pcapng`. Expected: dumpcap prints `Capturing on 'usbmon<N>'`, and the file exists and grows.
17. Start the re-attach watcher as in step 8. Expected: the watcher runs and prints no error.
18. In aquasuite, press the update button of the System page and wait. Do not touch the VM until aquasuite ends. Expected: aquasuite reports the update as finished without an error, and the watcher logs two re-attaches.
19. Stop the watcher and the capture, shut down the VM, then run `sudo chown $USER re/captures/firmware-update-run2-en.pcapng`. Expected: the capture file is larger than 150 KB.
20. Read report 1 again as in step 4. Expected: it prints `03fe00` (firmware 1022, English).
21. Summarize the capture as in step 12, with the file `re/captures/firmware-update-run2-en.pcapng` and the output `re/captures/firmware-update-run2-en.json`. Expected: the summary prints the same length of feature report 32 as run 1, and 108 SET_REPORT lines for report 32.

## Record

Steps 4 to 12 are run 1. Steps 13 to 21 are run 2.

| Step | Result |
|---|---|
| 1 | |
| 2 | |
| 3 | |
| 4 | |
| 5 | |
| 6 | |
| 7 | |
| 8 | |
| 9 | |
| 10 | |
| 11 | |
| 12 | |
| 13 | |
| 14 | |
| 15 | |
| 16 | |
| 17 | |
| 18 | |
| 19 | |
| 20 | |
| 21 | |
