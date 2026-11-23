//! A simulated Leakshield for tests. It starts from the fw1022 dumps and records every request.
//! It is behind the Cargo feature `sim`, which only `[dev-dependencies]` enable.

use std::collections::{BTreeMap, HashMap};
use std::io;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use crate::device::{BulkTransport, DeviceError, HidTransport};
use crate::firmware::frame::{CHUNK_SIZE, Chunk, REPORT32_ID, chunks};
use crate::firmware::image::Image;
use crate::report1::Report1;
use crate::report2::Report2;
use crate::report3::{REPORT3_ID, REPORT3_SIZE, Report3};
use crate::report8::REPORT8_ID;

const REPORT1: &[u8] = include_bytes!("../../../tests/fixtures/leakshield/fw1022/report1.bin");
const REPORT3: &[u8] = include_bytes!("../../../tests/fixtures/leakshield/fw1022/report3.bin");
const REPORT5: &[u8] = include_bytes!("../../../tests/fixtures/leakshield/fw1022/report5.bin");
const REPORT8: &[u8] = include_bytes!("../../../tests/fixtures/leakshield/fw1022/report8.bin");

/// One request that reached the simulated device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// `read()` of an input report.
    ReadInput,
    /// `HIDIOCGFEATURE`.
    GetFeature {
        id: u8,
        len: usize,
    },
    /// `HIDIOCSFEATURE`.
    SetFeature {
        id: u8,
        len: usize,
    },
    WriteOutput(Vec<u8>),
    BulkWrite(Vec<u8>),
}

#[derive(Default)]
struct Faults {
    /// Feature writes fail with an I/O error. The device keeps its report.
    feature: bool,
    output: bool,
    bulk: bool,
    /// The device stores this byte instead of the written one, and fixes the CRC.
    tamper_next_set_feature: Option<(usize, u8)>,
    disconnect_after_set_feature: bool,
    /// Report 3 offset that the device increments on every feature read.
    volatile_offset: Option<usize>,
}

/// What the simulated bootloader does with the write of one chunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkBehavior {
    Accept,
    /// The write returns an error at once.
    FailFast,
    /// The write sleeps in real time inside the call, then succeeds.
    Slow(Duration),
    /// The write succeeds and the chunk is lost.
    Drop,
}

/// The bootloader mode of a simulated device (design D12).
#[derive(Debug, Clone)]
pub struct BootloaderConfig {
    /// The time to enter and to leave the bootloader. The sim has no clock: the harness that
    /// re-enumerates the device waits these times.
    pub enter: Duration,
    pub leave: Duration,
    /// The behavior of each chunk, by its index in the image. A missing index is `Accept`.
    pub chunks: HashMap<usize, ChunkBehavior>,
    /// The image that the bootloader expects.
    pub image: Image,
    /// The firmware version that the device reports after a complete image.
    pub new_firmware: u16,
}

struct Bootloader {
    config: BootloaderConfig,
    expected: Vec<Chunk>,
    received: BTreeMap<u32, [u8; CHUNK_SIZE]>,
    active: bool,
}

impl Bootloader {
    fn complete(&self) -> bool {
        self.expected
            .iter()
            .all(|c| self.received.get(&c.address) == Some(&c.data))
    }

    /// The report 1 that the bootloader sends by itself: 64 bytes, byte 2 = 1 and firmware 1,
    /// as measured on the reference device. The identity fields stay 0 until gate 1.6.
    fn report1() -> Vec<u8> {
        let mut report = vec![0; 64];
        report[0] = 1;
        report[2] = 1;
        report[14] = 1;
        report
    }
}

struct State {
    /// The Mode that the device returns to when an Action ends (1 to 3).
    base_mode: Option<i16>,
    report1: Vec<u8>,
    report3: Vec<u8>,
    report5: Vec<u8>,
    report8: Vec<u8>,
    connected: bool,
    faults: Faults,
    log: Vec<Request>,
    outputs: Vec<Vec<u8>>,
    bulk: Vec<Vec<u8>>,
    bootloader: Option<Bootloader>,
}

/// Report 2 commands that the simulated device acts on (`docs/protocol/leakshield.md`):
/// `set_mode` and the codes that it takes, and the others.
const COMMAND_RESTART: u32 = 1;
const COMMAND_FACTORY_DEFAULTS: u32 = 3;
const COMMAND_RESET_ALARM: u32 = 11;
const COMMAND_SET_MODE: u32 = 12;
const COMMAND_SET_CLOCK: u32 = 37000;

impl State {
    fn edit_report1(&mut self, edit: impl FnOnce(&mut Report1)) {
        let mut report = Report1::decode(&self.report1).expect("the device holds a valid report 1");
        edit(&mut report);
        self.report1 = report.encode();
    }

    fn in_bootloader(&self) -> bool {
        self.bootloader.as_ref().is_some_and(|b| b.active)
    }

    /// A device without a bootloader mode ignores the restart command.
    fn enter_bootloader(&mut self) {
        if let Some(bootloader) = &mut self.bootloader {
            bootloader.active = true;
            bootloader.received.clear();
        }
    }

    /// The commands that the bootloader takes. A complete image starts the new firmware, and an
    /// incomplete one leaves the device in the bootloader (inferred, `docs/re/firmware-update.md`).
    fn bootloader_command(&mut self, command: Report2) -> Result<(), DeviceError> {
        let bootloader = self
            .bootloader
            .as_mut()
            .expect("the device is in the bootloader");
        match (command.command, command.parameter) {
            (COMMAND_RESTART, 0) if bootloader.complete() => {
                bootloader.active = false;
                bootloader.received.clear();
                let firmware = bootloader.config.new_firmware;
                self.edit_report1(|r| r.firmware_version = firmware);
                Ok(())
            }
            (COMMAND_RESTART, _) => Ok(()),
            _ => Err(io_error("the bootloader takes only the restart command")),
        }
    }

    fn finish_action(&mut self) {
        if let Some(mode) = self.base_mode {
            self.edit_report1(|r| r.mode = mode);
        }
    }

    /// What the device does with a command: Mode and Action codes, cancel (code 0) and the clock.
    fn apply(&mut self, command: Report2) {
        match (command.command, command.parameter) {
            (COMMAND_RESTART, 1) => self.enter_bootloader(),
            (COMMAND_SET_MODE, 0) => self.finish_action(),
            (COMMAND_SET_MODE, code @ 1..=3) => {
                self.base_mode = Some(code as i16);
                // A running Action keeps the report until it ends.
                if !(4..=7).contains(&Report1::decode(&self.report1).map_or(0, |r| r.mode)) {
                    self.edit_report1(|r| r.mode = code as i16);
                }
            }
            (COMMAND_SET_MODE, code @ 4..=7) => {
                let current = Report1::decode(&self.report1).map_or(0, |r| r.mode);
                if (1..=3).contains(&current) {
                    self.base_mode = Some(current);
                }
                self.edit_report1(|r| r.mode = code as i16);
            }
            // A bit that is still set in `alarm_bits` stays latched: the device sets it again.
            (COMMAND_RESET_ALARM, _) => {
                self.edit_report1(|r| r.alarm_bits_latched = r.alarm_bits);
            }
            // The settings that the device returns to are the fw1022 dump.
            (COMMAND_FACTORY_DEFAULTS, _) => self.report3 = REPORT3.to_vec(),
            (COMMAND_SET_CLOCK, seconds) => self.edit_report1(|r| {
                r.device_clock = seconds;
                r.status_bits |= 1 << 2;
            }),
            _ => {}
        }
    }
}

/// A shared handle: clones see the same device, so a test keeps one while the code under test
/// holds another.
#[derive(Clone)]
pub struct SimDevice(Arc<Mutex<State>>);

fn io_error(message: &str) -> DeviceError {
    DeviceError::Io(io::Error::new(
        io::ErrorKind::InvalidInput,
        message.to_owned(),
    ))
}

impl SimDevice {
    /// The reference device: the fw1022 dumps.
    pub fn fw1022() -> Self {
        Self(Arc::new(Mutex::new(State {
            base_mode: None,
            report1: REPORT1.to_vec(),
            report3: REPORT3.to_vec(),
            report5: REPORT5.to_vec(),
            report8: REPORT8.to_vec(),
            connected: true,
            faults: Faults::default(),
            log: Vec::new(),
            outputs: Vec::new(),
            bulk: Vec::new(),
            bootloader: None,
        })))
    }

    /// Gives the device a bootloader: report 2 command 1 with parameter 1 enters it.
    pub fn with_bootloader(self, config: BootloaderConfig) -> Self {
        self.state().bootloader = Some(Bootloader {
            expected: chunks(&config.image),
            received: BTreeMap::new(),
            active: false,
            config,
        });
        self
    }

    pub fn in_bootloader(&self) -> bool {
        self.state().in_bootloader()
    }

    /// The chunks that the bootloader holds now, in address order.
    pub fn received_chunks(&self) -> Vec<Chunk> {
        let state = self.state();
        state
            .bootloader
            .iter()
            .flat_map(|b| &b.received)
            .map(|(&address, &data)| Chunk { address, data })
            .collect()
    }

    /// Changes the behavior of one chunk, as when the cause of a failure is gone.
    pub fn set_chunk_behavior(&self, index: usize, behavior: ChunkBehavior) {
        if let Some(bootloader) = &mut self.state().bootloader {
            bootloader.config.chunks.insert(index, behavior);
        }
    }

    /// The write of a feature report 32 in the bootloader. A slow chunk sleeps without the lock.
    fn bootloader_set_feature(&self, report: &[u8]) -> Result<(), DeviceError> {
        let chunk = Chunk::parse_frame(report).map_err(|e| io_error(&e.to_string()))?;
        let behavior = {
            let state = self.state();
            let bootloader = state
                .bootloader
                .as_ref()
                .expect("the device is in the bootloader");
            let index = bootloader
                .expected
                .iter()
                .position(|c| c.address == chunk.address);
            index
                .and_then(|i| bootloader.config.chunks.get(&i))
                .cloned()
                .unwrap_or(ChunkBehavior::Accept)
        };
        match behavior {
            ChunkBehavior::FailFast => return Err(io_error("chunk write failed")),
            ChunkBehavior::Drop => return Ok(()),
            ChunkBehavior::Slow(delay) => std::thread::sleep(delay),
            ChunkBehavior::Accept => {}
        }
        let mut state = self.state();
        if let Some(bootloader) = &mut state.bootloader {
            bootloader.received.insert(chunk.address, chunk.data);
        }
        Ok(())
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.0.lock().unwrap()
    }

    pub fn requests(&self) -> Vec<Request> {
        self.state().log.clone()
    }

    pub fn clear_requests(&self) {
        self.state().log.clear();
    }

    /// Output reports (report 2) that the device accepted.
    pub fn output_reports(&self) -> Vec<Vec<u8>> {
        self.state().outputs.clone()
    }

    /// The accepted output reports, decoded.
    pub fn commands(&self) -> Vec<Report2> {
        self.output_reports()
            .iter()
            .map(|r| Report2::decode(r).expect("the device accepts valid report 2 only"))
            .collect()
    }

    /// Feed messages that the device accepted.
    pub fn bulk_messages(&self) -> Vec<Vec<u8>> {
        self.state().bulk.clone()
    }

    /// The report 1 that the device holds now.
    pub fn report1(&self) -> Report1 {
        Report1::decode(&self.state().report1).expect("the device holds a valid report 1")
    }

    pub fn report3(&self) -> Vec<u8> {
        self.state().report3.clone()
    }

    pub fn report5(&self) -> Vec<u8> {
        self.state().report5.clone()
    }

    pub fn report8(&self) -> Vec<u8> {
        self.state().report8.clone()
    }

    /// Changes report 3 behind the daemon's back, as another tool or the device button does.
    /// The CRC stays valid.
    pub fn change_report3(&self, offset: usize, value: &[u8]) {
        let mut state = self.state();
        let mut report =
            Report3::decode(&state.report3).expect("the device holds a valid report 3");
        report.set_bytes(offset, value);
        state.report3 = report.encode();
    }

    pub fn edit_report1(&self, edit: impl FnOnce(&mut Report1)) {
        let mut state = self.state();
        let mut report =
            Report1::decode(&state.report1).expect("the device holds a valid report 1");
        edit(&mut report);
        state.report1 = report.encode();
    }

    /// Sets `alarm_bits` and latches every bit that is set: the device holds a latched bit until
    /// command 11 (`reset_alarm`), also after the bit in `alarm_bits` goes. `edit_report1` does
    /// not latch.
    pub fn set_alarm_bits(&self, bits: u32) {
        self.edit_report1(|r| {
            r.alarm_bits = bits;
            r.alarm_bits_latched |= bits;
        });
    }

    /// The running Action ends: report 1 shows the Mode again.
    pub fn finish_action(&self) {
        let mut state = self.state();
        state.finish_action();
    }

    pub fn disconnect(&self) {
        self.state().connected = false;
    }

    pub fn reconnect(&self) {
        self.state().connected = true;
    }

    /// Output reports fail with an I/O error.
    pub fn fail_output(&self, fail: bool) {
        self.state().faults.output = fail;
    }

    /// Feature writes (report 3) fail with an I/O error.
    pub fn fail_feature_writes(&self, fail: bool) {
        self.state().faults.feature = fail;
    }

    /// Bulk writes fail with an I/O error.
    pub fn fail_bulk(&self, fail: bool) {
        self.state().faults.bulk = fail;
    }

    /// The next report 3 write stores `value` at `offset` instead of the written byte.
    pub fn tamper_next_set_feature(&self, offset: usize, value: u8) {
        self.state().faults.tamper_next_set_feature = Some((offset, value));
    }

    /// The device vanishes right after it stores the next feature write.
    pub fn disconnect_after_set_feature(&self) {
        self.state().faults.disconnect_after_set_feature = true;
    }

    /// The device changes the byte at `offset` of report 3 on each feature read.
    pub fn make_volatile(&self, offset: usize) {
        self.state().faults.volatile_offset = Some(offset);
    }

    fn connected(state: &State) -> Result<(), DeviceError> {
        if state.connected {
            Ok(())
        } else {
            Err(DeviceError::Disconnected)
        }
    }
}

impl HidTransport for SimDevice {
    fn read_input(&self, _timeout: Duration) -> Result<Vec<u8>, DeviceError> {
        let mut state = self.state();
        state.log.push(Request::ReadInput);
        Self::connected(&state)?;
        if state.in_bootloader() {
            return Ok(Bootloader::report1());
        }
        Ok(state.report1.clone())
    }

    fn get_feature(&self, id: u8, len: usize) -> Result<Vec<u8>, DeviceError> {
        let mut state = self.state();
        state.log.push(Request::GetFeature { id, len });
        Self::connected(&state)?;
        if state.in_bootloader() {
            return Err(io_error("the bootloader has no feature report to read"));
        }
        if let (3, Some(offset)) = (id, state.faults.volatile_offset) {
            let mut report = Report3::decode(&state.report3).expect("valid report 3");
            let next = report.bytes_at(offset, 1)[0].wrapping_add(1);
            report.set_bytes(offset, &[next]);
            state.report3 = report.encode();
        }
        let bytes = match id {
            3 => &state.report3,
            5 => &state.report5,
            8 => &state.report8,
            _ => return Err(io_error("no such feature report")),
        };
        if bytes.len() != len {
            return Err(io_error("wrong feature report size"));
        }
        Ok(bytes.clone())
    }

    fn set_feature(&self, report: &[u8]) -> Result<(), DeviceError> {
        let mut state = self.state();
        let id = report.first().copied().unwrap_or(0);
        state.log.push(Request::SetFeature {
            id,
            len: report.len(),
        });
        Self::connected(&state)?;
        if state.faults.feature {
            return Err(io_error("feature write failed"));
        }
        if state.in_bootloader() {
            drop(state);
            if id != REPORT32_ID {
                return Err(io_error("the bootloader takes only report 32"));
            }
            return self.bootloader_set_feature(report);
        }
        match id {
            REPORT3_ID => {
                if report.len() != REPORT3_SIZE {
                    return Err(io_error("wrong report 3 size"));
                }
                let mut stored =
                    Report3::decode(report).map_err(|_| io_error("report 3 has a bad CRC"))?;
                if let Some((offset, value)) = state.faults.tamper_next_set_feature.take() {
                    stored.set_bytes(offset, &[value]);
                }
                state.report3 = stored.encode();
            }
            REPORT8_ID => state.report8 = report.to_vec(),
            _ => return Err(io_error("report is not writable")),
        }
        if std::mem::take(&mut state.faults.disconnect_after_set_feature) {
            state.connected = false;
        }
        Ok(())
    }

    fn write_output(&self, report: &[u8]) -> Result<(), DeviceError> {
        let mut state = self.state();
        state.log.push(Request::WriteOutput(report.to_vec()));
        Self::connected(&state)?;
        if state.faults.output {
            return Err(io_error("output report failed"));
        }
        let command = Report2::decode(report).map_err(|_| io_error("bad report 2"))?;
        if state.in_bootloader() {
            state.bootloader_command(command)?;
            state.outputs.push(report.to_vec());
            return Ok(());
        }
        state.outputs.push(report.to_vec());
        state.apply(command);
        Ok(())
    }
}

impl BulkTransport for SimDevice {
    fn write(&self, data: &[u8]) -> Result<(), DeviceError> {
        let mut state = self.state();
        state.log.push(Request::BulkWrite(data.to_vec()));
        Self::connected(&state)?;
        if state.in_bootloader() {
            return Err(io_error("the bootloader has no bulk interface"));
        }
        if state.faults.bulk {
            return Err(io_error("bulk write failed"));
        }
        state.bulk.push(data.to_vec());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::firmware::bootloader::{Identity, restart_into_bootloader, restart_plain};
    use crate::firmware::image::tests::image_of;

    const NEW_FIRMWARE: u16 = 1023;

    fn image() -> Image {
        image_of(&[(0x4000, (0..3000).map(|i| (i % 251) as u8).collect())])
    }

    fn device(chunks: &[(usize, ChunkBehavior)]) -> SimDevice {
        SimDevice::fw1022().with_bootloader(BootloaderConfig {
            enter: Duration::ZERO,
            leave: Duration::ZERO,
            chunks: chunks.iter().cloned().collect(),
            image: image(),
            new_firmware: NEW_FIRMWARE,
        })
    }

    fn enter(sim: &SimDevice) {
        sim.write_output(&restart_into_bootloader()).unwrap();
    }

    fn send(sim: &SimDevice, indices: impl IntoIterator<Item = usize>) {
        let all = chunks(&image());
        for i in indices {
            sim.set_feature(&all[i].frame()).unwrap();
        }
    }

    #[test]
    fn sim_enters_bootloader_on_restart_1() {
        let sim = device(&[]);
        assert!(!sim.in_bootloader());
        enter(&sim);
        assert!(sim.in_bootloader());
        let commands = sim.commands();
        assert_eq!((commands[0].command, commands[0].parameter), (1, 1));
        // A device without a bootloader mode ignores the command.
        let plain = SimDevice::fw1022();
        plain.write_output(&restart_into_bootloader()).unwrap();
        assert!(!plain.in_bootloader());
    }

    #[test]
    fn sim_bootloader_report1_is_the_placeholder() {
        let sim = device(&[]);
        enter(&sim);
        let report = sim.read_input(Duration::ZERO).unwrap();
        assert_eq!(report.len(), 64);
        assert_eq!((report[0], report[2], report[13], report[14]), (1, 1, 0, 1));
        assert_eq!(report.iter().filter(|&&b| b != 0).count(), 3);
        assert!(Identity::from_report1(&report).is_bootloader());
        assert!(sim.get_feature(3, 750).is_err());
    }

    #[test]
    fn sim_rejects_bad_crc() {
        let sim = device(&[]);
        enter(&sim);
        let mut frame = chunks(&image())[0].frame();
        frame[1041] ^= 1;
        assert!(sim.set_feature(&frame).is_err());
        assert!(sim.received_chunks().is_empty());
    }

    #[test]
    fn sim_rejects_wrong_length() {
        let sim = device(&[]);
        enter(&sim);
        let frame = chunks(&image())[0].frame();
        assert!(sim.set_feature(&frame[..1041]).is_err());
        assert!(sim.received_chunks().is_empty());
    }

    #[test]
    fn sim_slow_chunk_takes_400_ms() {
        let sim = device(&[(1, ChunkBehavior::Slow(Duration::from_millis(400)))]);
        enter(&sim);
        let start = Instant::now();
        send(&sim, [1]);
        assert!(start.elapsed() >= Duration::from_millis(400));
        assert_eq!(sim.received_chunks().len(), 1);
    }

    #[test]
    fn sim_dropped_chunk_is_lost() {
        let sim = device(&[(1, ChunkBehavior::Drop)]);
        enter(&sim);
        send(&sim, 0..3);
        let addresses: Vec<u32> = sim.received_chunks().iter().map(|c| c.address).collect();
        assert_eq!(addresses, [0x4000, 0x4800]);
        sim.write_output(&restart_plain()).unwrap();
        assert!(sim.in_bootloader());
    }

    #[test]
    fn sim_complete_image_returns_new_firmware() {
        let sim = device(&[]);
        enter(&sim);
        send(&sim, 0..3);
        sim.write_output(&restart_plain()).unwrap();
        assert!(!sim.in_bootloader());
        assert_eq!(sim.report1().firmware_version, NEW_FIRMWARE);
        assert_eq!(sim.read_input(Duration::ZERO).unwrap().len(), 837);
        assert!(sim.received_chunks().is_empty());
    }

    #[test]
    fn sim_incomplete_image_stays_in_bootloader() {
        let sim = device(&[]);
        enter(&sim);
        send(&sim, 0..2);
        sim.write_output(&restart_plain()).unwrap();
        assert!(sim.in_bootloader());
        assert_eq!(sim.report1().firmware_version, 1022);
        assert_eq!(sim.read_input(Duration::ZERO).unwrap().len(), 64);
    }

    #[test]
    fn sim_second_flash_recovers() {
        let sim = device(&[(1, ChunkBehavior::FailFast)]);
        enter(&sim);
        send(&sim, [0]);
        let failed = chunks(&image())[1].frame();
        assert!(sim.set_feature(&failed).is_err());
        sim.write_output(&restart_plain()).unwrap();
        assert!(sim.in_bootloader());

        sim.set_chunk_behavior(1, ChunkBehavior::Accept);
        send(&sim, 0..3);
        sim.write_output(&restart_plain()).unwrap();
        assert!(!sim.in_bootloader());
        assert_eq!(sim.report1().firmware_version, NEW_FIRMWARE);
    }
}
