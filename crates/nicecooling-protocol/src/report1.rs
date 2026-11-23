use crate::error::{ReportError, check_id, check_size};
use crate::wire::{Reader, Writer};

pub const REPORT1_ID: u8 = 1;
pub const REPORT1_SIZE: usize = 837;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClockTime {
    pub second: u8,
    pub minute: u8,
    pub hour: u8,
    pub day: u8,
    pub month: u8,
    pub year: u16,
    pub day_of_year: u16,
    pub weekday: u8,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SensorSlot {
    pub unit_code: u8,
    pub name_index: u8,
    pub value: i16,
    pub minimum: i16,
    pub maximum: i16,
    pub average: i16,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RefillEntry {
    pub time: u32,
    pub duration: u32,
    pub pump_turns: u32,
    pub volume: i32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EventEntry {
    pub time: u32,
    pub entry_id: u8,
    pub event_code: u8,
    pub mode: i16,
    pub procedure_state: i16,
    pub pressure: i16,
    pub pressure_leak_rate: i16,
    pub volume_leak_rate: i16,
    pub value_1: i16,
    pub value_2: i16,
}

/// Input report 1, decoded field by field. Every byte of the report belongs to a field, so
/// `encode` returns the bytes that `decode` read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report1 {
    pub structure_number: u16,
    pub serial: [u16; 2],
    pub hardware_revision: u16,
    pub device_kind: u16,
    pub bootloader_version: u16,
    pub firmware_version: u16,
    pub status_bits: u32,
    pub feature_bits: u8,
    pub uptime: u32,
    pub power_up_count: u32,
    pub total_runtime: u32,
    pub last_button_press: u16,
    pub display_language: u8,
    pub device_clock: u32,
    pub local_time: ClockTime,
    pub settings_dirty: u8,
    /// The device profile number, 1 to 4.
    pub active_profile: u8,
    pub raw_adc_values: [u16; 4],
    pub sensors: [SensorSlot; 20],
    pub supply_voltage: i16,
    pub pump_speed_reading: i16,
    pub pump_current_draw: i16,
    pub daily_pump_duration: i16,
    pub pump_speed_setpoint: i16,
    pub fill_valve_setpoint: i16,
    pub release_valve_setpoint: i16,
    pub pressure_raw_32: i32,
    pub pressure_raw_32_filtered: i32,
    pub pressure_raw: i16,
    pub pressure_offset: i16,
    pub pressure: i16,
    pub membrane_state: i16,
    pub pump_head_pressure: i16,
    pub pressure_low: i16,
    pub pressure_setpoint: i16,
    pub pressure_high: i16,
    pub mode: i16,
    pub procedure_state: i16,
    pub pressure_leak_rate_average: i16,
    pub volume_leak_rate_average: i16,
    pub pressure_leak_rate: i16,
    pub volume_leak_rate: i16,
    pub air_volume: i16,
    pub water_volume: i16,
    pub reservoir_volume: i16,
    pub fill_level: i16,
    pub progress_1: i16,
    pub progress_2: i16,
    pub pump_runtime: u32,
    pub pump_short_cycles: u32,
    pub pump_revolution_count: u32,
    pub fill_valve_cycles: u32,
    pub release_valve_cycles: u32,
    pub leak_rate_statistics: [i16; 6],
    pub refill_history: [RefillEntry; 7],
    pub event_list: [EventEntry; 15],
    pub alarm_bits: u32,
    pub alarm_bits_latched: u32,
    pub debug: [i16; 32],
}

impl Report1 {
    pub fn decode(bytes: &[u8]) -> Result<Self, ReportError> {
        check_size(bytes, REPORT1_SIZE)?;
        check_id(bytes, REPORT1_ID)?;
        let mut r = Reader::new(bytes);
        r.u8();
        let structure_number = r.u16();
        let serial = [r.u16(), r.u16()];
        let hardware_revision = r.u16();
        let device_kind = r.u16();
        let bootloader_version = r.u16();
        let firmware_version = r.u16();
        let status_bits = r.u32();
        let feature_bits = r.u8();
        let uptime = r.u32();
        let power_up_count = r.u32();
        let total_runtime = r.u32();
        let last_button_press = r.u16();
        let display_language = r.u8();
        let device_clock = r.u32();
        let local_time = ClockTime {
            second: r.u8(),
            minute: r.u8(),
            hour: r.u8(),
            day: r.u8(),
            month: r.u8(),
            year: r.u16(),
            day_of_year: r.u16(),
            weekday: r.u8(),
        };
        let settings_dirty = r.u8();
        let active_profile = r.u8();
        let raw_adc_values = r.array(Reader::u16);
        let sensors = r.array(|r| SensorSlot {
            unit_code: r.u8(),
            name_index: r.u8(),
            value: r.i16(),
            minimum: r.i16(),
            maximum: r.i16(),
            average: r.i16(),
        });
        let report = Self {
            structure_number,
            serial,
            hardware_revision,
            device_kind,
            bootloader_version,
            firmware_version,
            status_bits,
            feature_bits,
            uptime,
            power_up_count,
            total_runtime,
            last_button_press,
            display_language,
            device_clock,
            local_time,
            settings_dirty,
            active_profile,
            raw_adc_values,
            sensors,
            supply_voltage: r.i16(),
            pump_speed_reading: r.i16(),
            pump_current_draw: r.i16(),
            daily_pump_duration: r.i16(),
            pump_speed_setpoint: r.i16(),
            fill_valve_setpoint: r.i16(),
            release_valve_setpoint: r.i16(),
            pressure_raw_32: r.i32(),
            pressure_raw_32_filtered: r.i32(),
            pressure_raw: r.i16(),
            pressure_offset: r.i16(),
            pressure: r.i16(),
            membrane_state: r.i16(),
            pump_head_pressure: r.i16(),
            pressure_low: r.i16(),
            pressure_setpoint: r.i16(),
            pressure_high: r.i16(),
            mode: r.i16(),
            procedure_state: r.i16(),
            pressure_leak_rate_average: r.i16(),
            volume_leak_rate_average: r.i16(),
            pressure_leak_rate: r.i16(),
            volume_leak_rate: r.i16(),
            air_volume: r.i16(),
            water_volume: r.i16(),
            reservoir_volume: r.i16(),
            fill_level: r.i16(),
            progress_1: r.i16(),
            progress_2: r.i16(),
            pump_runtime: r.u32(),
            pump_short_cycles: r.u32(),
            pump_revolution_count: r.u32(),
            fill_valve_cycles: r.u32(),
            release_valve_cycles: r.u32(),
            leak_rate_statistics: r.array(Reader::i16),
            refill_history: r.array(|r| RefillEntry {
                time: r.u32(),
                duration: r.u32(),
                pump_turns: r.u32(),
                volume: r.i32(),
            }),
            event_list: r.array(|r| EventEntry {
                time: r.u32(),
                entry_id: r.u8(),
                event_code: r.u8(),
                mode: r.i16(),
                procedure_state: r.i16(),
                pressure: r.i16(),
                pressure_leak_rate: r.i16(),
                volume_leak_rate: r.i16(),
                value_1: r.i16(),
                value_2: r.i16(),
            }),
            alarm_bits: r.u32(),
            alarm_bits_latched: r.u32(),
            debug: r.array(Reader::i16),
        };
        Ok(report)
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer(Vec::with_capacity(REPORT1_SIZE));
        w.u8(REPORT1_ID);
        w.u16(self.structure_number);
        self.serial.iter().for_each(|&v| w.u16(v));
        w.u16(self.hardware_revision);
        w.u16(self.device_kind);
        w.u16(self.bootloader_version);
        w.u16(self.firmware_version);
        w.u32(self.status_bits);
        w.u8(self.feature_bits);
        w.u32(self.uptime);
        w.u32(self.power_up_count);
        w.u32(self.total_runtime);
        w.u16(self.last_button_press);
        w.u8(self.display_language);
        w.u32(self.device_clock);
        let t = &self.local_time;
        for v in [t.second, t.minute, t.hour, t.day, t.month] {
            w.u8(v);
        }
        w.u16(t.year);
        w.u16(t.day_of_year);
        w.u8(t.weekday);
        w.u8(self.settings_dirty);
        w.u8(self.active_profile);
        self.raw_adc_values.iter().for_each(|&v| w.u16(v));
        for s in &self.sensors {
            w.u8(s.unit_code);
            w.u8(s.name_index);
            for v in [s.value, s.minimum, s.maximum, s.average] {
                w.i16(v);
            }
        }
        for v in [
            self.supply_voltage,
            self.pump_speed_reading,
            self.pump_current_draw,
            self.daily_pump_duration,
            self.pump_speed_setpoint,
            self.fill_valve_setpoint,
            self.release_valve_setpoint,
        ] {
            w.i16(v);
        }
        w.i32(self.pressure_raw_32);
        w.i32(self.pressure_raw_32_filtered);
        for v in [
            self.pressure_raw,
            self.pressure_offset,
            self.pressure,
            self.membrane_state,
            self.pump_head_pressure,
            self.pressure_low,
            self.pressure_setpoint,
            self.pressure_high,
            self.mode,
            self.procedure_state,
            self.pressure_leak_rate_average,
            self.volume_leak_rate_average,
            self.pressure_leak_rate,
            self.volume_leak_rate,
            self.air_volume,
            self.water_volume,
            self.reservoir_volume,
            self.fill_level,
            self.progress_1,
            self.progress_2,
        ] {
            w.i16(v);
        }
        for v in [
            self.pump_runtime,
            self.pump_short_cycles,
            self.pump_revolution_count,
            self.fill_valve_cycles,
            self.release_valve_cycles,
        ] {
            w.u32(v);
        }
        self.leak_rate_statistics.iter().for_each(|&v| w.i16(v));
        for e in &self.refill_history {
            w.u32(e.time);
            w.u32(e.duration);
            w.u32(e.pump_turns);
            w.i32(e.volume);
        }
        for e in &self.event_list {
            w.u32(e.time);
            w.u8(e.entry_id);
            w.u8(e.event_code);
            for v in [
                e.mode,
                e.procedure_state,
                e.pressure,
                e.pressure_leak_rate,
                e.volume_leak_rate,
                e.value_1,
                e.value_2,
            ] {
                w.i16(v);
            }
        }
        w.u32(self.alarm_bits);
        w.u32(self.alarm_bits_latched);
        self.debug.iter().for_each(|&v| w.i16(v));
        w.0
    }

    /// The serial number in its text form, `<first>-<second>`.
    pub fn serial_text(&self) -> String {
        format!("{}-{}", self.serial[0], self.serial[1])
    }
}
