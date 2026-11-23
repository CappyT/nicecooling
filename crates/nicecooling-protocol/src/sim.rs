//! A simulated Leakshield for tests. It starts from the fw1022 dumps and records every request.
//! It is behind the Cargo feature `sim`, which only `[dev-dependencies]` enable.

use std::io;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use crate::device::{BulkTransport, DeviceError, HidTransport};
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
    output: bool,
    bulk: bool,
    /// The device stores this byte instead of the written one, and fixes the CRC.
    tamper_next_set_feature: Option<(usize, u8)>,
    disconnect_after_set_feature: bool,
    /// Report 3 offset that the device increments on every feature read.
    volatile_offset: Option<usize>,
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
}

/// Report 2 commands that the simulated device acts on (`docs/protocol/leakshield.md`):
/// `set_mode` and the codes that it takes, and the others.
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

    fn finish_action(&mut self) {
        if let Some(mode) = self.base_mode {
            self.edit_report1(|r| r.mode = mode);
        }
    }

    /// What the device does with a command: Mode and Action codes, cancel (code 0) and the clock.
    fn apply(&mut self, command: Report2) {
        match (command.command, command.parameter) {
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
        })))
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
        Ok(state.report1.clone())
    }

    fn get_feature(&self, id: u8, len: usize) -> Result<Vec<u8>, DeviceError> {
        let mut state = self.state();
        state.log.push(Request::GetFeature { id, len });
        Self::connected(&state)?;
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
        if state.faults.bulk {
            return Err(io_error("bulk write failed"));
        }
        state.bulk.push(data.to_vec());
        Ok(())
    }
}
