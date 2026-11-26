//! One tracing setup for every binary.
//!
//! Filter with `VICTORY_LOG` (falls back to `RUST_LOG`), e.g.
//! `VICTORY_LOG=victory_loader=trace,info`. Debug and trace output exist
//! only in builds with the `diagnostics` feature; a plain release build
//! logs info and above whatever the filter asks for.
//!
//! Logging never makes a thread wait: a line is formatted on the thread that
//! logs it and handed to a queue without blocking (a full queue drops the
//! line and counts it); one thread of ours writes the queue out. The threads
//! that record and submit the GPU's work log too, and a write to a file or a
//! console can stall for milliseconds.

use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::time::{Duration, Instant};

use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// The most verbose level this build can log.
const CAP: LevelFilter = if cfg!(feature = "diagnostics") {
    LevelFilter::TRACE
} else {
    LevelFilter::INFO
};

/// Lines waiting for the writer at most; past it a line is dropped.
const QUEUED: usize = 16 * 1024;

/// Lines handed to the queue and lines written out (for [`drain`]); lines
/// dropped because the queue was full.
static SENT: AtomicU64 = AtomicU64::new(0);
static WRITTEN: AtomicU64 = AtomicU64::new(0);
static DROPPED: AtomicU64 = AtomicU64::new(0);

pub fn init(default_level: &str) {
    let filter = std::env::var("VICTORY_LOG")
        .or_else(|_| std::env::var("RUST_LOG"))
        .ok()
        .and_then(|v| EnvFilter::try_new(v).ok())
        .unwrap_or_else(|| EnvFilter::new(default_level));
    let Some(queue) = Queue::start() else {
        // No writer thread: written where the line is logged, as before.
        let _ = tracing_subscriber::registry()
            .with(CAP)
            .with(filter)
            .with(tracing_subscriber::fmt::layer().with_target(true).with_writer(std::io::stderr))
            .try_init();
        return;
    };
    let _ = tracing_subscriber::registry()
        .with(CAP)
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_target(true).with_writer(queue))
        .try_init();
}

/// Waits up to `most` for the lines queued so far to be written (a crash
/// report or the process's end; the writer thread dies with the process).
pub fn drain(most: Duration) {
    let target = SENT.load(Ordering::Acquire);
    let until = Instant::now() + most;
    while WRITTEN.load(Ordering::Acquire) < target && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Where the lines go: standard error, or the file `VICTORY_LOG_FILE` names
/// (appended to): a program that starts ours and keeps its output (Nsight
/// Systems' `nsys launch`) leaves the log where the scripts read it.
fn sink() -> Box<dyn Write + Send> {
    if let Some(path) = std::env::var_os("VICTORY_LOG_FILE").filter(|p| !p.is_empty()) {
        match std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            Ok(file) => return Box::new(file),
            Err(e) => eprintln!("VICTORY_LOG_FILE {}: {e}; logging to standard error", path.to_string_lossy()),
        }
    }
    Box::new(std::io::stderr())
}

/// The queue's sending end, given to the formatting layer.
struct Queue {
    tx: SyncSender<Vec<u8>>,
}

impl Queue {
    fn start() -> Option<Queue> {
        let (tx, rx) = sync_channel::<Vec<u8>>(QUEUED);
        let sink = sink();
        std::thread::Builder::new()
            .name("victory-log".into())
            .spawn(move || write_out(rx, sink))
            .ok()?;
        Some(Queue { tx })
    }
}

/// The writer thread: lines in order, buffered, flushed whenever the queue
/// is empty; a note of the lines dropped since the last one.
fn write_out(rx: Receiver<Vec<u8>>, sink: Box<dyn Write + Send>) {
    let mut out = std::io::BufWriter::with_capacity(64 * 1024, sink);
    while let Ok(line) = rx.recv() {
        let _ = out.write_all(&line);
        WRITTEN.fetch_add(1, Ordering::Release);
        // More waiting: write them before flushing.
        while let Ok(line) = rx.try_recv() {
            let _ = out.write_all(&line);
            WRITTEN.fetch_add(1, Ordering::Release);
        }
        let dropped = DROPPED.swap(0, Ordering::Relaxed);
        if dropped > 0 {
            let _ = writeln!(out, "(logging: {dropped} lines dropped, the queue was full)");
        }
        let _ = out.flush();
    }
}

impl<'a> MakeWriter<'a> for Queue {
    type Writer = Line<'a>;
    fn make_writer(&'a self) -> Line<'a> {
        Line { tx: &self.tx, buf: Vec::with_capacity(256) }
    }
}

/// One event's text, sent to the queue when the layer is done with it.
struct Line<'a> {
    tx: &'a SyncSender<Vec<u8>>,
    buf: Vec<u8>,
}

impl Write for Line<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for Line<'_> {
    fn drop(&mut self) {
        if self.buf.is_empty() {
            return;
        }
        match self.tx.try_send(std::mem::take(&mut self.buf)) {
            Ok(()) => {
                SENT.fetch_add(1, Ordering::Release);
            }
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
                DROPPED.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}
