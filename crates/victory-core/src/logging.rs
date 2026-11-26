//! One tracing setup for every binary.
//!
//! Filter with `VICTORY_LOG` (falls back to `RUST_LOG`), e.g.
//! `VICTORY_LOG=victory_loader=trace,info`. Debug and trace output exist
//! only in builds with the `diagnostics` feature; a plain release build
//! logs info and above whatever the filter asks for.

use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// The most verbose level this build can log.
const CAP: LevelFilter = if cfg!(feature = "diagnostics") {
    LevelFilter::TRACE
} else {
    LevelFilter::INFO
};

pub fn init(default_level: &str) {
    let filter = std::env::var("VICTORY_LOG")
        .or_else(|_| std::env::var("RUST_LOG"))
        .ok()
        .and_then(|v| EnvFilter::try_new(v).ok())
        .unwrap_or_else(|| EnvFilter::new(default_level));
    let _ = tracing_subscriber::registry()
        .with(CAP)
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_writer(writer()),
        )
        .try_init();
}

/// Standard error, or the file `VICTORY_LOG_FILE` names (appended to): a
/// program that starts ours and keeps its output (Nsight Systems' `nsys
/// launch`) leaves the log where the scripts read it.
fn writer() -> tracing_subscriber::fmt::writer::BoxMakeWriter {
    use tracing_subscriber::fmt::writer::BoxMakeWriter;
    if let Some(path) = std::env::var_os("VICTORY_LOG_FILE").filter(|p| !p.is_empty()) {
        match std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            Ok(file) => return BoxMakeWriter::new(std::sync::Mutex::new(file)),
            Err(e) => eprintln!("VICTORY_LOG_FILE {}: {e}; logging to standard error", path.to_string_lossy()),
        }
    }
    BoxMakeWriter::new(std::io::stderr)
}
