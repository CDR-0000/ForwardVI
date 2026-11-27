//! Frame times as they happen: every present's time since the one before.
//!
//! - `VICTORY_FRAME_GRAPH=1`: a small window of its own draws one bar per
//!   frame (newest on the right, a log scale from 16 ms to 2 s), green up to
//!   100 ms, yellow up to 250, red above, under the median of the last two
//!   seconds (and its frames a second), the last frame, the worst 1% of the
//!   last ten seconds and the draws a frame. A plain "frames in the last
//!   second" read 1-2 fps when one stall of a second fell in it, at 200-300
//!   ms frames. Its own thread paints it four times a second from the kept
//!   times; the presenting thread only adds two numbers under a lock.
//! - `VICTORY_FRAME_LOG=<file>`: one line per frame, flushed every second,
//!   for comparing runs frame by frame: `frame,ms,draws`, then what came
//!   between that present and the one before, of the frame line's numbers
//!   ([`crate::host::frame_counters`]): ms preparing submissions, waiting
//!   for the card in them, copying back; ms the submitting thread worked;
//!   ms the card had nothing of ours queued, of that while the submitting
//!   thread worked, and that thread stood for the card; MB of title memory
//!   copied up and copied back. (Those counters go back to 0 with the
//!   frame-rate line once a second: that frame's numbers lack what came
//!   before it in the frame.)

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// Frame times kept (ms), oldest first: enough for the graph's width and
/// ten seconds of fast frames.
const KEPT: usize = 600;

struct Times {
    last: Option<Instant>,
    frames: u64,
    ms: VecDeque<f32>,
    /// Draws of each kept frame, and the draw count at the last present.
    draws: VecDeque<u32>,
    drawn: u64,
    log: Option<std::io::BufWriter<std::fs::File>>,
    flushed: Option<Instant>,
    /// [`counters_now`] at the last present (with the frame log only).
    counters: [u64; COUNTERS],
}

static TIMES: Mutex<Times> = Mutex::new(Times {
    last: None,
    frames: 0,
    ms: VecDeque::new(),
    draws: VecDeque::new(),
    drawn: 0,
    log: None,
    flushed: None,
    counters: [0; COUNTERS],
});

/// The frame log's columns after `frame,ms,draws`.
const COUNTERS: usize = 9;
const COLUMNS: &str =
    "prep_ms,gpu_wait_ms,back_ms,busy_ms,card_idle_ms,idle_behind_us_ms,stood_ms,up_mb,back_mb";

/// [`crate::host::frame_counters`] now (zeros without a host).
fn counters_now() -> [u64; COUNTERS] {
    #[cfg(windows)]
    {
        crate::host::frame_counters()
    }
    #[cfg(not(windows))]
    {
        [0; COUNTERS]
    }
}

fn times() -> std::sync::MutexGuard<'static, Times> {
    TIMES.lock().unwrap_or_else(|e| e.into_inner())
}

/// A frame was presented: its time since the last is kept (and logged).
pub(crate) fn presented() {
    use std::io::Write;
    static LOG: OnceLock<Option<String>> = OnceLock::new();
    let log = LOG.get_or_init(|| std::env::var("VICTORY_FRAME_LOG").ok().filter(|p| !p.trim().is_empty()));
    let now = Instant::now();
    let drawn = crate::draw::drawn();
    let mut t = times();
    let draws = drawn.saturating_sub(std::mem::replace(&mut t.drawn, drawn)).min(u64::from(u32::MAX)) as u32;
    // What came since the last present (a counter now below its last value went back to 0).
    let counters = if log.is_some() { counters_now() } else { [0; COUNTERS] };
    let before = std::mem::replace(&mut t.counters, counters);
    let came: [u64; COUNTERS] =
        std::array::from_fn(|i| if counters[i] >= before[i] { counters[i] - before[i] } else { counters[i] });
    let Some(last) = t.last.replace(now) else {
        drop(t);
        start_graph();
        return;
    };
    let ms = (now - last).as_secs_f64() * 1000.0;
    t.frames += 1;
    if t.ms.len() == KEPT {
        t.ms.pop_front();
        t.draws.pop_front();
    }
    t.ms.push_back(ms as f32);
    t.draws.push_back(draws);
    if let Some(path) = log {
        if t.log.is_none() && t.flushed.is_none() {
            t.flushed = Some(now);
            match std::fs::File::create(path) {
                Ok(f) => {
                    let mut w = std::io::BufWriter::new(f);
                    let _ = writeln!(w, "frame,ms,draws,{COLUMNS}");
                    t.log = Some(w);
                }
                Err(e) => tracing::warn!(error = %e, path, "no frame log"),
            }
        }
        let frame = t.frames;
        if let Some(w) = t.log.as_mut() {
            let ms_of = |us: u64| us as f64 / 1e3;
            let mb_of = |bytes: u64| bytes as f64 / 1e6;
            let _ = writeln!(
                w,
                "{frame},{ms:.2},{draws},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2}",
                ms_of(came[0]),
                ms_of(came[1]),
                ms_of(came[2]),
                ms_of(came[3]),
                ms_of(came[4]),
                ms_of(came[5]),
                ms_of(came[6]),
                mb_of(came[7]),
                mb_of(came[8])
            );
        }
        if t.flushed.is_some_and(|f| now - f >= std::time::Duration::from_secs(1)) {
            t.flushed = Some(now);
            if let Some(w) = t.log.as_mut() {
                let _ = w.flush();
            }
        }
    }
}

/// What the caption and the graph say.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Stats {
    /// The last frame (ms).
    pub last: f32,
    /// The median of the frames of the last two seconds (ms).
    pub median: f32,
    /// The worst 1% of the frames of the last ten seconds (ms): the time
    /// that 99% of them were shorter than.
    pub worst: f32,
}

/// [`Stats`] of `ms` (oldest first), or none without a frame.
pub(crate) fn stats_of(ms: &[f32]) -> Option<Stats> {
    let &last = ms.last()?;
    // The newest frames that make up `secs` seconds (at least one).
    let recent = |secs: f32| {
        let mut sum = 0.0;
        let mut n = 0;
        for &m in ms.iter().rev() {
            n += 1;
            sum += m;
            if sum >= secs * 1000.0 {
                break;
            }
        }
        let mut v: Vec<f32> = ms[ms.len() - n..].to_vec();
        v.sort_by(f32::total_cmp);
        v
    };
    let two = recent(2.0);
    let median = if two.len() % 2 == 1 {
        two[two.len() / 2]
    } else {
        (two[two.len() / 2 - 1] + two[two.len() / 2]) / 2.0
    };
    let ten = recent(10.0);
    let worst = ten[((ten.len() as f32 * 0.99).ceil() as usize).clamp(1, ten.len()) - 1];
    Some(Stats { last, median, worst })
}

/// The median draws a frame of the newest `n` frames of `draws`.
fn median_draws(draws: &[u32], n: usize) -> u32 {
    let mut v: Vec<u32> = draws[draws.len().saturating_sub(n.max(1))..].to_vec();
    if v.is_empty() {
        return 0;
    }
    v.sort_unstable();
    v[v.len() / 2]
}

/// Starts the graph window's thread once, with `VICTORY_FRAME_GRAPH=1`.
fn start_graph() {
    static STARTED: OnceLock<()> = OnceLock::new();
    if std::env::var_os("VICTORY_FRAME_GRAPH").is_none_or(|v| v != "1") {
        return;
    }
    STARTED.get_or_init(|| {
        let _ = std::thread::Builder::new().name("frame graph".into()).spawn(|| {
            if let Err(e) = graph::run() {
                tracing::warn!(error = %e, "no frame graph window");
            }
        });
    });
}

/// The graph window (GDI, its own thread and message loop).
#[cfg(windows)]
mod graph {
    use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::core::{PCWSTR, w};

    const WIDTH: i32 = 600;
    const HEIGHT: i32 = 220;
    const TIMER: usize = 1;
    /// The scale's ends (ms), logarithmic.
    const LOW: f32 = 16.0;
    const HIGH: f32 = 2000.0;

    fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
        COLORREF(u32::from(r) | u32::from(g) << 8 | u32::from(b) << 16)
    }

    /// The height (pixels from the top of `plot`) of `ms` on the scale.
    fn y_of(ms: f32, top: i32, bottom: i32) -> i32 {
        let f = (ms.clamp(LOW, HIGH).ln() - LOW.ln()) / (HIGH.ln() - LOW.ln());
        bottom - ((bottom - top) as f32 * f).round() as i32
    }

    pub(super) fn run() -> windows::core::Result<()> {
        const CLASS: PCWSTR = w!("ForwardVI frame graph");
        // SAFETY: registers our class and creates a window of it on this
        // thread, which then runs its messages.
        unsafe {
            let instance = GetModuleHandleW(None)?;
            let class = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(proc_),
                hInstance: instance.into(),
                lpszClassName: CLASS,
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                ..Default::default()
            };
            RegisterClassExW(&class);
            let mut rect = RECT { left: 0, top: 0, right: WIDTH, bottom: HEIGHT };
            let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_VISIBLE;
            AdjustWindowRect(&mut rect, style, false)?;
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                CLASS,
                w!("ForwardVI frame times"),
                style,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                rect.right - rect.left,
                rect.bottom - rect.top,
                None,
                None,
                Some(instance.into()),
                None,
            )?;
            SetTimer(Some(hwnd), TIMER, 250, None);
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        Ok(())
    }

    extern "system" fn proc_(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        // SAFETY: our window's messages, on its thread.
        unsafe {
            match message {
                WM_TIMER => {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    LRESULT(0)
                }
                WM_ERASEBKGND => LRESULT(1),
                WM_PAINT => {
                    paint(hwnd);
                    LRESULT(0)
                }
                WM_DESTROY => {
                    PostQuitMessage(0);
                    LRESULT(0)
                }
                _ => DefWindowProcW(hwnd, message, wparam, lparam),
            }
        }
    }

    /// Draws into a bitmap, then onto the window in one copy (no flicker).
    unsafe fn paint(hwnd: HWND) {
        // SAFETY: GDI objects made here and released here; the window's
        // device context between BeginPaint and EndPaint.
        unsafe {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut client = RECT::default();
            let _ = GetClientRect(hwnd, &mut client);
            let (w, h) = (client.right, client.bottom);
            let mem = CreateCompatibleDC(Some(dc));
            let bmp = CreateCompatibleBitmap(dc, w, h);
            let old = SelectObject(mem, bmp.into());
            let back = CreateSolidBrush(rgb(18, 18, 22));
            FillRect(mem, &client, back);
            let _ = DeleteObject(back.into());

            let (top, bottom, left, right) = (34, h - 6, 44, w - 6);
            SetBkMode(mem, TRANSPARENT);
            // Lines at these times, labelled.
            let grid = CreatePen(PS_SOLID, 1, rgb(55, 55, 65));
            let old_pen = SelectObject(mem, grid.into());
            SetTextColor(mem, rgb(140, 140, 150));
            for ms in [16.7f32, 33.3, 100.0, 250.0, 500.0, 1000.0] {
                let y = y_of(ms, top, bottom);
                let _ = MoveToEx(mem, left, y, None);
                let _ = LineTo(mem, right, y);
                let label: Vec<u16> = format!("{ms:.0}").encode_utf16().collect();
                let _ = TextOutW(mem, 4, y - 8, &label);
            }
            SelectObject(mem, old_pen);
            let _ = DeleteObject(grid.into());

            let (ms, draws): (Vec<f32>, Vec<u32>) = {
                let t = super::times();
                (t.ms.iter().copied().collect(), t.draws.iter().copied().collect())
            };
            // One bar a frame, two pixels wide, newest on the right.
            let fits = ((right - left) / 2).max(1) as usize;
            let shown = &ms[ms.len().saturating_sub(fits)..];
            let brushes = [rgb(70, 200, 90), rgb(230, 200, 60), rgb(230, 70, 60)].map(|c| CreateSolidBrush(c));
            for (i, &m) in shown.iter().enumerate() {
                let x = right - 2 * (shown.len() - i) as i32;
                let brush = brushes[if m <= 100.0 { 0 } else if m <= 250.0 { 1 } else { 2 }];
                let bar = RECT { left: x, top: y_of(m, top, bottom), right: x + 2, bottom };
                FillRect(mem, &bar, brush);
            }
            for b in brushes {
                let _ = DeleteObject(b.into());
            }

            SetTextColor(mem, rgb(230, 230, 235));
            let text = match super::stats_of(&ms) {
                Some(s) => {
                    // The frames of the last two seconds, for the draws.
                    let mut n = 0;
                    let mut sum = 0.0;
                    for &m in ms.iter().rev() {
                        n += 1;
                        sum += m;
                        if sum >= 2000.0 {
                            break;
                        }
                    }
                    format!(
                        "{:.1} fps (median {:.0} ms, last 2 s)   last {:.0} ms   worst 1% {:.0} ms (10 s)   {} draws/frame",
                        1000.0 / s.median.max(0.001),
                        s.median,
                        s.last,
                        s.worst,
                        super::median_draws(&draws, n)
                    )
                }
                None => "waiting for frames".to_owned(),
            };
            let text: Vec<u16> = text.encode_utf16().collect();
            let _ = TextOutW(mem, 8, 8, &text);

            let _ = BitBlt(dc, 0, 0, w, h, Some(mem), 0, 0, SRCCOPY);
            SelectObject(mem, old);
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(mem);
            let _ = EndPaint(hwnd, &ps);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_median_is_of_the_last_two_seconds() {
        // Ten seconds of 1,000 ms frames, then 2 s of 100 ms ones.
        let mut ms = vec![1000.0f32; 10];
        ms.extend([100.0; 20]);
        let s = stats_of(&ms).unwrap();
        assert_eq!(s.last, 100.0);
        assert_eq!(s.median, 100.0);
    }

    #[test]
    fn one_stall_does_not_set_the_median_but_sets_the_worst() {
        let mut ms = vec![200.0f32; 30];
        ms[25] = 1500.0;
        let s = stats_of(&ms).unwrap();
        assert_eq!(s.median, 200.0);
        assert_eq!(s.worst, 1500.0);
    }

    #[test]
    fn median_draws_of_the_newest_frames() {
        assert_eq!(median_draws(&[], 5), 0);
        assert_eq!(median_draws(&[10, 2000, 3000, 1000], 3), 2000);
        assert_eq!(median_draws(&[7], 0), 7);
    }

    #[test]
    fn a_few_frames_are_enough() {
        assert_eq!(stats_of(&[]), None);
        let s = stats_of(&[300.0]).unwrap();
        assert_eq!((s.last, s.median, s.worst), (300.0, 300.0, 300.0));
        let s = stats_of(&[100.0, 300.0]).unwrap();
        assert_eq!(s.median, 200.0);
    }
}
