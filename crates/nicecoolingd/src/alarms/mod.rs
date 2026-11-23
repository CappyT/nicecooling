//! Alarm responses and the poweroff (design D13). The tick evaluates the Alarms (core stage 3).
//! This module turns the active ones into instances with an alarm response, runs the countdown
//! of a poweroff and calls logind.

pub mod logind;
pub mod poweroff;
pub mod response;
