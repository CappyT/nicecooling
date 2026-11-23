use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use nicecoolingd::authz::PolkitAuthorizer;
use nicecoolingd::daemon::{self, Options, Paths};
use nicecoolingd::dbus;
use nicecoolingd::snapshot::{RestoreOutcome, restore_bios};
use nicecoolingd::tick_runner::TokioClock;
use nicecoolingd::watchdog::SystemdNotifier;

struct Args {
    restore_bios: bool,
    paths: Paths,
}

/// `--sysfs-root`, `--state-dir`, `--boards-dir` and `--calibration-dir` point the daemon at a
/// fake tree. The unit does not use them.
fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        restore_bios: false,
        paths: Paths::default(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut path = |name: &str| {
            it.next()
                .map(std::path::PathBuf::from)
                .ok_or(format!("{name} needs a path"))
        };
        match arg.as_str() {
            "--restore-bios" => args.restore_bios = true,
            "--sysfs-root" => args.paths.sysfs_root = path(&arg)?,
            "--state-dir" => args.paths.state_dir = path(&arg)?,
            "--boards-dir" => args.paths.boards_dir = path(&arg)?,
            "--calibration-dir" => args.paths.calibration_dir = path(&arg)?,
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(args)
}

fn restore(paths: &Paths) -> ExitCode {
    match restore_bios(paths) {
        Ok(RestoreOutcome::NoLease) => ExitCode::SUCCESS,
        Ok(RestoreOutcome::Restored(report)) if report.matched => ExitCode::SUCCESS,
        Ok(RestoreOutcome::Restored(report)) => {
            eprintln!(
                "nicecoolingd: the restore does not match the snapshot: {:?}",
                report.problems
            );
            ExitCode::FAILURE
        }
        Ok(RestoreOutcome::Fallback { channels, failed }) => {
            eprintln!(
                "nicecoolingd: no readable snapshot, {channels} Channels set to manual mode at raw 255 (state-fallback), {failed} writes failed"
            );
            if failed == 0 {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(e) => {
            eprintln!("nicecoolingd: restore failed: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn start_api(daemon: &daemon::Daemon) -> zbus::Result<(zbus::Connection, dbus::Served)> {
    let connection = zbus::connection::Builder::system().build().await?;
    let authorizer = PolkitAuthorizer::new(&connection).await?;
    let served = dbus::serve(&daemon.hub, &connection, Arc::new(authorizer)).await?;
    Ok((connection, served))
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("nicecoolingd: {e}");
            return ExitCode::from(2);
        }
    };
    if args.restore_bios {
        return restore(&args.paths);
    }
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("nicecoolingd: cannot start the runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    let code = runtime.block_on(serve(args));
    // A source stuck in a blocking device read would hold the runtime drop up to
    // `TimeoutStopSec`. `ExecStopPost` restores the snapshot only after the process exits.
    runtime.shutdown_timeout(Duration::from_secs(1));
    code
}

async fn serve(args: Args) -> ExitCode {
    let options = Options::new(args.paths);
    let mut daemon =
        match daemon::start(&options, Arc::new(TokioClock::new()), SystemdNotifier).await {
            Ok(daemon) => daemon,
            Err(e) => {
                eprintln!("nicecoolingd: start failed: {e}");
                return ExitCode::FAILURE;
            }
        };
    // A bus that fails does not stop the fan control: the Channels keep their Curves.
    let _api = match start_api(&daemon).await {
        Ok(api) => Some(api),
        Err(e) => {
            eprintln!("nicecoolingd: the D-Bus API did not start: {e}");
            None
        }
    };
    let _ = sd_notify::notify(&[sd_notify::NotifyState::Ready]);
    let mut term = match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
        Ok(term) => term,
        Err(e) => {
            eprintln!("nicecoolingd: cannot watch SIGTERM: {e}");
            return ExitCode::FAILURE;
        }
    };
    tokio::select! {
        () = daemon.run(None) => {}
        _ = term.recv() => {}
        _ = tokio::signal::ctrl_c() => {}
    }
    daemon.hub.flush_now();
    // `ExecStopPost` restores the BIOS snapshot.
    ExitCode::SUCCESS
}
