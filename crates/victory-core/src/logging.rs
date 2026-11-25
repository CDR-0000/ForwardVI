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
    use tracing_subscriber::Layer;
    // Spans of target "profile" are for the frame trace alone: the log
    // lines leave them out.
    let not_profile = tracing_subscriber::filter::filter_fn(|m| m.target() != "profile");
    let registry = tracing_subscriber::registry().with(CAP).with(filter).with(
        tracing_subscriber::fmt::layer()
            .with_target(true)
            .with_writer(std::io::stderr)
            .with_filter(not_profile),
    );
    // With the `profile` feature: those spans go to a trace file for Perfetto
    // (and nothing else does, so the log's events do not flood it).
    #[cfg(feature = "profile")]
    let registry = registry.with(profile_layer());
    let _ = registry.try_init();
}

/// The frame trace (`profile` feature): spans of target "profile" go to the
/// file `VICTORY_PROFILE` names (default `profile.json` in the working
/// folder), in the Chrome trace format Perfetto (ui.perfetto.dev) opens. The
/// file is flushed every two seconds, so a run that is killed still leaves a
/// trace (Perfetto reads one whose last bracket is missing).
#[cfg(feature = "profile")]
fn profile_layer<S>() -> impl tracing_subscriber::Layer<S>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a> + Send + Sync,
{
    use tracing_subscriber::Layer;
    let path = std::env::var("VICTORY_PROFILE").unwrap_or_else(|_| "profile.json".into());
    let (layer, guard) = tracing_chrome::ChromeLayerBuilder::new()
        .file(&path)
        .include_args(true)
        .build();
    let guard = std::sync::Arc::new(std::sync::Mutex::new(guard));
    std::thread::Builder::new()
        .name("profile flush".into())
        .spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(2));
                guard.lock().unwrap_or_else(|e| e.into_inner()).flush();
            }
        })
        .ok();
    eprintln!("profile      frame trace in {path} (open it at ui.perfetto.dev)");
    layer.with_filter(tracing_subscriber::filter::filter_fn(|m| m.target() == "profile"))
}
