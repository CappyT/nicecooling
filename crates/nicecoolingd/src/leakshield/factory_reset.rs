//! The factory reset: backup, the `factory_defaults` command, read-back of report 3. The write
//! worker runs it, so it does not overlap a write.

use std::time::Duration;

use nicecooling_protocol::access::{Access, WriteRefusal};
use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::report1::Report1;
use nicecooling_protocol::report2::Report2;

use super::backup::Reason;
use super::blocking;
use super::write::{WriteEnv, WriteError, Written, device_error, take_backup};

/// The device applies the defaults after it takes the command. How long it needs is not
/// measured: the read-back waits this long first.
const SETTLE: Duration = Duration::from_millis(500);

/// `None` when the registry of the firmware has no `factory_defaults` command.
pub fn command(registry: &Registry) -> Option<Report2> {
    registry
        .command("factory_defaults")
        .map(|c| Report2::new(c.code, 0))
}

/// The state from before the reset stays in the backup of the result. The reply holds report 3
/// as the device returns it after the reset.
pub fn factory_reset(
    env: &WriteEnv<'_>,
    report1: &Report1,
    now_us: u64,
) -> Result<Written, WriteError> {
    if let Some(reason) = Access::new(env.registry, report1).device_reason() {
        return Err(WriteError::Refused(WriteRefusal::ReadOnly(reason)));
    }
    let command = command(env.registry).ok_or(WriteError::NotSupported)?;
    let backup = take_backup(env, report1, Reason::BeforeWrite, now_us)?;
    env.io
        .send_command(command)
        .map_err(|e| device_error(e, Some(&backup)))?;
    blocking(|| std::thread::sleep(SETTLE));
    let report3 = env
        .io
        .read_report3()
        .map_err(|e| device_error(e, Some(&backup)))?;
    Ok(Written { backup, report3 })
}
