//! Log file and crash reports, kept on this computer only (nothing is uploaded).
//!
//! - `cadsync.<date>.log`: daily rolling log, 14 days kept.
//! - `crash-<time>.txt`: written by the panic hook with the message, location
//!   and backtrace. The UI mentions it on the next start.
//!
//! Location: Windows `%LOCALAPPDATA%\org.frc.cadsync\logs`, macOS
//! `~/Library/Logs/org.frc.cadsync`.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

const ACK_FILE: &str = "crash-reports-seen";

/// Keeps the background log writer alive; dropping it flushes the file.
pub struct LogGuard(#[allow(dead_code)] Option<WorkerGuard>);

pub fn init(dir: &Path) -> LogGuard {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| "cadsync=info".into());
    let file = std::fs::create_dir_all(dir).ok().and_then(|_| {
        RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix("cadsync")
            .filename_suffix("log")
            .max_log_files(14)
            .build(dir)
            .ok()
    });
    let (file_layer, guard) = match file {
        Some(appender) => {
            let (writer, guard) = tracing_appender::non_blocking(appender);
            (Some(fmt::layer().with_ansi(false).with_writer(writer)), Some(guard))
        }
        None => (None, None),
    };
    let _ = tracing_subscriber::registry().with(filter).with(fmt::layer()).with(file_layer).try_init();
    install_panic_hook(dir.to_path_buf());
    tracing::info!(version = env!("CARGO_PKG_VERSION"), os = std::env::consts::OS, "cadsync starting");
    LogGuard(guard)
}

fn install_panic_hook(dir: PathBuf) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let report = crash_report(info);
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let path = dir.join(format!("crash-{stamp}.txt"));
        let _ = std::fs::write(&path, &report);
        tracing::error!(report = %path.display(), "cadsync crashed: {}", panic_message(info));
        previous(info);
    }));
}

fn panic_message(info: &std::panic::PanicHookInfo) -> String {
    info.payload()
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".into())
}

fn crash_report(info: &std::panic::PanicHookInfo) -> String {
    format!(
        "cadsync crash report\n\
         version: {}\nos: {} {}\ntime: {}\nthread: {}\nlocation: {}\nmessage: {}\n\nbacktrace:\n{}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        chrono::Local::now().to_rfc3339(),
        std::thread::current().name().unwrap_or("unnamed"),
        info.location().map(|l| l.to_string()).unwrap_or_default(),
        panic_message(info),
        std::backtrace::Backtrace::force_capture(),
    )
}

/// The newest crash report the user hasn't been told about yet, if any.
/// Marks it as seen.
pub fn take_new_crash_report(dir: &Path) -> Option<PathBuf> {
    let seen = std::fs::metadata(dir.join(ACK_FILE)).and_then(|m| m.modified()).ok();
    let newest = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("crash-"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .filter(|(t, _)| seen.is_none_or(|s| *t > s))
        .max_by_key(|(t, _)| *t)
        .map(|(_, p)| p);
    if newest.is_some() {
        let _ = std::fs::write(dir.join(ACK_FILE), format!("{:?}", SystemTime::now()));
    }
    newest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_each_crash_once() {
        let dir = tempfile::tempdir().unwrap();
        assert!(take_new_crash_report(dir.path()).is_none());
        std::fs::write(dir.path().join("crash-20261018-120000.txt"), "boom").unwrap();
        let p = take_new_crash_report(dir.path()).expect("new crash found");
        assert!(p.ends_with("crash-20261018-120000.txt"));
        assert!(take_new_crash_report(dir.path()).is_none(), "already seen");
    }
}
