//! The card's time in the frame trace (`profile` feature): a lane named
//! "GPU" beside the processor's threads, one span per submission sent
//! ahead, from when the card began it to when it finished, and once a
//! frame the counters of what the frame made (textures uploaded, upload
//! buffers, pipelines, dropped draws).
//!
//! Each submission gets a list before its own lists and one after, each a
//! single timestamp query; the one after also resolves the two into a
//! read-back buffer. The lists are recorded once, a pair per slot, and
//! sent again whenever their slot comes round: a slot is used again only
//! after its submission's fence has passed. The card's ticks are put on
//! the trace's clock with the queue's clock calibration (a card tick and
//! the processor's performance counter taken together), and the trace's
//! start is the performance counter the logging setup read just before it
//! made the trace (`VICTORY_PROFILE_T0`).
//!
//! The events go to a file of their own beside the trace
//! (`<VICTORY_PROFILE>.gpu.json`, one event a line), which the frame
//! report reads with it.

use std::collections::VecDeque;
use std::io::Write;
use std::sync::Mutex;
use std::sync::atomic::Ordering::Relaxed;

use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC;
use windows::core::Interface;

/// Slots: submissions timed at once. Late wait keeps at most 128 lists in
/// the air, a few submissions' worth; a slot not free yet goes untimed.
const SLOTS: usize = 1024;
/// The lane's thread id in the trace (no thread of ours has it).
const LANE: u32 = 99_999;

struct State {
    _allocator: ID3D12CommandAllocator,
    _heap: ID3D12QueryHeap,
    _readback: ID3D12Resource,
    ticks: *const u64,
    /// Per slot: the list before a submission and the list after it.
    lists: Vec<(ID3D12CommandList, ID3D12CommandList)>,
    /// Slots not in use.
    free: Vec<usize>,
    /// Slots sent: the slot, the fence value behind its submission, the
    /// submission's number.
    sent: VecDeque<(usize, u64, u64)>,
    out: std::io::BufWriter<std::fs::File>,
    gpu_hz: f64,
    qpc_hz: f64,
    t0: i64,
}

// SAFETY: D3D12 objects are free-threaded; the read-back pointer is read
// only under the lock, after the card has written it.
unsafe impl Send for State {}

static STATE: Mutex<Option<Option<State>>> = Mutex::new(None);

#[link(name = "kernel32")]
unsafe extern "system" {
    fn QueryPerformanceFrequency(frequency: *mut i64) -> i32;
}

fn make(device: &ID3D12Device, queue: &ID3D12CommandQueue) -> Option<State> {
    let t0: i64 = std::env::var("VICTORY_PROFILE_T0").ok()?.parse().ok()?;
    let path = format!("{}.gpu.json", std::env::var("VICTORY_PROFILE").unwrap_or_else(|_| "profile.json".into()));
    let mut out = std::io::BufWriter::new(std::fs::File::create(&path).ok()?);
    let _ = writeln!(out, "[");
    let _ = writeln!(out, r#"{{"name":"thread_name","ph":"M","pid":1,"tid":{LANE},"args":{{"name":"GPU"}}}},"#);
    // SAFETY: plain creations and queries on live objects.
    unsafe {
        let heap: ID3D12QueryHeap = {
            let mut h: Option<ID3D12QueryHeap> = None;
            device
                .CreateQueryHeap(
                    &D3D12_QUERY_HEAP_DESC {
                        Type: D3D12_QUERY_HEAP_TYPE_TIMESTAMP,
                        Count: (2 * SLOTS) as u32,
                        NodeMask: 0,
                    },
                    &mut h,
                )
                .ok()?;
            h?
        };
        let readback: ID3D12Resource = {
            let mut r: Option<ID3D12Resource> = None;
            device
                .CreateCommittedResource(
                    &D3D12_HEAP_PROPERTIES { Type: D3D12_HEAP_TYPE_READBACK, ..Default::default() },
                    D3D12_HEAP_FLAG_NONE,
                    &D3D12_RESOURCE_DESC {
                        Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
                        Width: (2 * SLOTS * 8) as u64,
                        Height: 1,
                        DepthOrArraySize: 1,
                        MipLevels: 1,
                        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                        Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
                        ..Default::default()
                    },
                    D3D12_RESOURCE_STATE_COPY_DEST,
                    None,
                    &mut r,
                )
                .ok()?;
            r?
        };
        let mut ticks = std::ptr::null_mut();
        readback.Map(0, None, Some(&mut ticks)).ok()?;
        let allocator: ID3D12CommandAllocator = device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT).ok()?;
        let mut lists = Vec::with_capacity(SLOTS);
        for slot in 0..SLOTS {
            let begin: ID3D12GraphicsCommandList =
                device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None).ok()?;
            begin.EndQuery(&heap, D3D12_QUERY_TYPE_TIMESTAMP, (2 * slot) as u32);
            begin.Close().ok()?;
            let end: ID3D12GraphicsCommandList =
                device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None).ok()?;
            end.EndQuery(&heap, D3D12_QUERY_TYPE_TIMESTAMP, (2 * slot + 1) as u32);
            end.ResolveQueryData(&heap, D3D12_QUERY_TYPE_TIMESTAMP, (2 * slot) as u32, 2, &readback, (2 * slot * 8) as u64);
            end.Close().ok()?;
            lists.push((begin.cast().ok()?, end.cast().ok()?));
        }
        let gpu_hz = queue.GetTimestampFrequency().ok()? as f64;
        let mut qpc_hz = 0i64;
        QueryPerformanceFrequency(&mut qpc_hz);
        tracing::info!(path, "frame trace: the card's lane beside it");
        Some(State {
            _allocator: allocator,
            _heap: heap,
            _readback: readback,
            ticks: ticks.cast(),
            lists,
            free: (0..SLOTS).rev().collect(),
            sent: VecDeque::new(),
            out,
            gpu_hz,
            qpc_hz: qpc_hz as f64,
            t0,
        })
    }
}

/// The lists to send before and after a submission's own, and their slot;
/// none if the lane is off or every slot is in use.
pub(crate) fn around(device: &ID3D12Device, queue: &ID3D12CommandQueue) -> Option<(ID3D12CommandList, ID3D12CommandList, usize)> {
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let s = state.get_or_insert_with(|| make(device, queue)).as_mut()?;
    let slot = s.free.pop()?;
    let (begin, end) = s.lists[slot].clone();
    Some((begin, end, slot))
}

/// `slot`'s lists were sent around submission `number`, behind which the
/// queue signals `fence`.
pub(crate) fn sent(slot: usize, fence: u64, number: u64) {
    if let Some(Some(s)) = STATE.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        s.sent.push_back((slot, fence, number));
    }
}

/// Once a frame: the submissions the card has finished go into the lane,
/// and the frame's counters with them.
pub(crate) fn frame(queue: &ID3D12CommandQueue, fence: &ID3D12Fence) {
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let Some(Some(s)) = state.as_mut() else { return };
    // SAFETY: plain queries on live objects.
    let done = unsafe { fence.GetCompletedValue() };
    let (mut gpu_now, mut cpu_now) = (0u64, 0u64);
    // SAFETY: as above.
    if unsafe { queue.GetClockCalibration(&mut gpu_now, &mut cpu_now) }.is_err() {
        return;
    }
    let us = |qpc: f64| (qpc - s.t0 as f64) * 1e6 / s.qpc_hz;
    let gpu_us = |tick: u64| us(cpu_now as f64 + (tick as f64 - gpu_now as f64) * s.qpc_hz / s.gpu_hz);
    while let Some(&(slot, value, number)) = s.sent.front() {
        if value > done {
            break;
        }
        s.sent.pop_front();
        s.free.push(slot);
        // SAFETY: the card has run the slot's resolve (its fence passed).
        let (a, b) = unsafe { (s.ticks.add(2 * slot).read_volatile(), s.ticks.add(2 * slot + 1).read_volatile()) };
        if a == 0 || b < a {
            continue;
        }
        let (from, to) = (gpu_us(a), gpu_us(b));
        let _ = writeln!(
            s.out,
            r#"{{"name":"submission","ph":"X","pid":1,"tid":{LANE},"ts":{from:.1},"dur":{:.1},"args":{{"number":{number}}}}},"#,
            to - from
        );
    }
    // The frame's counters, since the start (the report takes differences).
    let now = us(cpu_now as f64);
    let (made, loaded) = crate::pipeline::library_counts();
    let _ = writeln!(
        s.out,
        r#"{{"name":"counters","ph":"C","pid":1,"tid":{LANE},"ts":{now:.1},"args":{{"heap_textures":{},"upload_buffers":{},"pipelines_made":{},"pipelines_loaded":{},"staging_made":{},"staging_reused":{},"dropped":{}}}}},"#,
        crate::draw::HEAP_TEXTURES.load(Relaxed),
        crate::host::MADE[0].load(Relaxed),
        made,
        loaded,
        crate::texture::SPARE_STATS[1].load(Relaxed),
        crate::texture::SPARE_STATS[0].load(Relaxed),
        crate::draw::dropped_count(),
    );
    let _ = s.out.flush();
}
