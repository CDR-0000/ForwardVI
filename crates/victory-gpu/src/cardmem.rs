//! The title's GPU memory on the card (`VICTORY_CARD_MEMORY=1`; off by
//! default): the nearest a PC comes to the console's one memory.
//!
//! A reservation the title asks for write-combined (memory the processor
//! writes and the GPU reads) is made of the card's own memory, which the
//! processor writes through Resizable BAR (a D3D12 GPU upload heap): the
//! title's stores land on the card, and the GPU reads them at the card's
//! speed. Buffers and windows of title memory in it are placed over it (no
//! host copy, no refresh, no snapshot, no copy-back; what shaders write is
//! still declared to the flights). Nothing watches its writes: a host copy
//! of it (a texture's staging, say) is copied whole each time.
//!
//! D3D12 takes no address for a mapping, and the title's GPU memory must
//! lie below 1 TiB (its ray-tracing structures, our descriptors); the free
//! address space above 1 TiB is reserved for the moment the heap is mapped
//! (`crate::host::fence_above_1_tib`), and the mapping lands under it.
//!
//! The processor reads this memory at some 42 MB/s (writes at 20 GB/s, both
//! measured 26 November): every read of ours is counted by where it is made
//! ([`read`], said once a second), to move to the GPU (stage 2 of the
//! roadmap's item 6).

use std::collections::HashMap;
use std::panic::Location;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC;

/// Whether the title's write-combined memory goes on the card.
pub(crate) fn on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_CARD_MEMORY").is_some_and(|v| v == "1"))
}

/// Whether the host offers GPU upload heaps (said once, at device creation).
static SUPPORTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The host's answer to whether GPU upload heaps are there.
pub(crate) fn set_supported(yes: bool) {
    SUPPORTED.store(yes, Ordering::Relaxed);
}

/// Whether list scratch memory is made on the card too (a GPU upload heap
/// the processor writes and the GPU copies into): fills of upload memory
/// from the title's memory on the card are then GPU copies
/// (`crate::host::HostList::card_fill`), not processor reads at 42 MB/s.
pub(crate) fn scratch_on_card() -> bool {
    on() && SUPPORTED.load(Ordering::Relaxed)
}

/// A reservation of the title's on the card: where the title sees it, its
/// bytes, the heap, one buffer over all of it and the buffer's GPU address.
struct Block {
    start: usize,
    len: usize,
    heap: ID3D12Heap,
    buffer: ID3D12Resource,
    gpu: u64,
}

// SAFETY: D3D12 objects are free-threaded.
unsafe impl Send for Block {}

static BLOCKS: Mutex<Vec<Block>> = Mutex::new(Vec::new());
/// The lowest start and highest end of the blocks: a quick "not ours".
static LO: AtomicUsize = AtomicUsize::new(usize::MAX);
static HI: AtomicUsize = AtomicUsize::new(0);

const LINE: usize = 64 << 10;

/// For the runtime: `size` bytes of the card's memory for a reservation the
/// title asked for write-combined, mapped for the processor between
/// `lowest` and `highest`; the address, or `None` (switch off, no device,
/// the host refused, or the mapping landed outside): the caller allocates
/// as before.
pub fn title_card_memory(size: usize, lowest: usize, highest: usize) -> Option<usize> {
    if !on() || size == 0 {
        return None;
    }
    let host = crate::host::host()?;
    let device = &host.device;
    let bytes = size.next_multiple_of(LINE);
    let started = std::time::Instant::now();
    let desc = D3D12_HEAP_DESC {
        SizeInBytes: bytes as u64,
        Properties: D3D12_HEAP_PROPERTIES { Type: D3D12_HEAP_TYPE_GPU_UPLOAD, ..Default::default() },
        Alignment: 0,
        Flags: D3D12_HEAP_FLAG_ALLOW_ONLY_BUFFERS,
    };
    // The address space above 1 TiB is taken from here until the mapping
    // is made: the driver picks the processor's address for the heap when
    // it is made (fencing the mapping alone left it at 1 TiB less 2 GB,
    // running past it, ForwardVI's run of 26 November).
    let fences = crate::host::fence_above_1_tib();
    let mut heap: Option<ID3D12Heap> = None;
    // SAFETY: plain creation.
    if let Err(e) = unsafe { device.CreateHeap(&desc, &mut heap) } {
        crate::host::unfence(&fences);
        tracing::warn!(mb = bytes >> 20, error = %e, "the title's write-combined memory not made on the card");
        return None;
    }
    let Some(heap) = heap else {
        crate::host::unfence(&fences);
        return None;
    };
    let buffer_desc = D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
        Width: bytes as u64,
        Height: 1,
        DepthOrArraySize: 1,
        MipLevels: 1,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
        Flags: D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
        ..Default::default()
    };
    let mut buffer: Option<ID3D12Resource> = None;
    // SAFETY: plain creation in a live heap.
    if let Err(e) = unsafe { device.CreatePlacedResource(&heap, 0, &buffer_desc, D3D12_RESOURCE_STATE_COMMON, None, &mut buffer) } {
        crate::host::unfence(&fences);
        tracing::warn!(mb = bytes >> 20, error = %e, "no buffer over the title's write-combined memory on the card");
        return None;
    }
    let Some(buffer) = buffer else {
        crate::host::unfence(&fences);
        return None;
    };
    let mut at: *mut std::ffi::c_void = std::ptr::null_mut();
    // SAFETY: a buffer of an upload-capable heap, whole range.
    let mapped = unsafe { buffer.Map(0, None, Some(&mut at)) };
    crate::host::unfence(&fences);
    if let Err(e) = mapped {
        tracing::warn!(error = %e, "the title's write-combined memory on the card not mapped");
        return None;
    }
    let start = at as usize;
    if start < lowest || start.saturating_add(bytes).saturating_sub(1) > highest {
        // SAFETY: mapped above.
        unsafe { buffer.Unmap(0, None) };
        tracing::warn!(
            at = format_args!("{start:#x}"),
            lowest = format_args!("{lowest:#x}"),
            highest = format_args!("{highest:#x}"),
            mb = bytes >> 20,
            "the title's write-combined memory on the card landed outside its addresses: allocated as before"
        );
        return None;
    }
    // SAFETY: a live buffer.
    let gpu = unsafe { buffer.GetGPUVirtualAddress() };
    crate::host::name(&buffer, || format!("title memory {start:#x}+{bytes:#x}, write-combined, on the card"));
    tracing::info!(
        at = format_args!("{start:#x}"),
        mb = bytes >> 20,
        fences = fences.len(),
        ms = format_args!("{:.1}", started.elapsed().as_secs_f64() * 1000.0),
        "the title's write-combined memory made on the card (VICTORY_CARD_MEMORY)"
    );
    LO.fetch_min(start, Ordering::Relaxed);
    HI.fetch_max(start + bytes, Ordering::Relaxed);
    BLOCKS.lock().unwrap_or_else(|e| e.into_inner()).push(Block { start, len: bytes, heap, buffer, gpu });
    Some(start)
}

/// Whether `at` lies in the title's memory on the card.
pub fn title_card_memory_holds(at: usize) -> bool {
    if at < LO.load(Ordering::Relaxed) || at >= HI.load(Ordering::Relaxed) {
        return false;
    }
    BLOCKS.lock().unwrap_or_else(|e| e.into_inner()).iter().any(|b| (b.start..b.start + b.len).contains(&at))
}

/// Whether `[at, at + len)` touches the title's memory on the card.
pub(crate) fn touches(at: usize, len: usize) -> bool {
    let (lo, hi) = (LO.load(Ordering::Relaxed), HI.load(Ordering::Relaxed));
    if at.saturating_add(len) <= lo || at >= hi {
        return false;
    }
    let end = at.saturating_add(len);
    BLOCKS.lock().unwrap_or_else(|e| e.into_inner()).iter().any(|b| at < b.start + b.len && end > b.start)
}

/// Whether `[at, at + len)` lies in the title's memory on the card whole:
/// always mapped (the title cannot unmap it), so it is read directly.
/// The kernel's reads of other processes' memory (`ReadProcessMemory`,
/// what reads of title memory go through elsewhere) refuse the card's
/// memory mapped for the processor: textures there read as zeros and were
/// never uploaded (black faces, 26 November).
pub(crate) fn holds(at: usize, len: usize) -> bool {
    if HI.load(Ordering::Relaxed) == 0 || at < LO.load(Ordering::Relaxed) || at >= HI.load(Ordering::Relaxed) {
        return false;
    }
    let end = at.saturating_add(len);
    BLOCKS.lock().unwrap_or_else(|e| e.into_inner()).iter().any(|b| b.start <= at && end <= b.start + b.len)
}

/// `[start, start + len)` placed over the title's memory on the card, if
/// it lies there whole.
pub(crate) fn place(start: u64, len: u64) -> Option<crate::inplace::Placed> {
    let (start, end) = (start as usize, (start + len.max(1)) as usize);
    if start < LO.load(Ordering::Relaxed) || start >= HI.load(Ordering::Relaxed) {
        return None;
    }
    let blocks = BLOCKS.lock().unwrap_or_else(|e| e.into_inner());
    let b = blocks.iter().find(|b| b.start <= start && end <= b.start + b.len)?;
    let bias = (start - b.start) as u64;
    Some(crate::inplace::Placed { buffer: b.buffer.clone(), heap: b.heap.clone(), gpu: b.gpu + bias, bias, told: true })
}

/// The card buffer and the offset in it of `[at, at + len)`, if it lies in
/// the title's memory on the card whole.
pub(crate) fn source(at: usize, len: usize) -> Option<(ID3D12Resource, u64)> {
    let end = at.saturating_add(len);
    let blocks = BLOCKS.lock().unwrap_or_else(|e| e.into_inner());
    let b = blocks.iter().find(|b| b.start <= at && end <= b.start + b.len)?;
    Some((b.buffer.clone(), (at - b.start) as u64))
}

/// Fills of upload memory that waited for the flights, by why they could
/// not be the card's: source not on the card, source partly on it,
/// destination not in a scratch chunk; and their bytes.
static BLOCKED_FILLS: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];

/// A fill of `len` bytes from `src` (into its list's scratch if
/// `in_scratch`) waited for the flights.
pub(crate) fn blocked_fill(src: usize, len: usize, _dst: usize, in_scratch: bool) {
    let why = if !touches(src, len) {
        0
    } else if source(src, len).is_none() {
        1
    } else if !in_scratch {
        2
    } else {
        // (On the card and in the scratch: only when the scratch is not on
        // the card; counted with the first.)
        0
    };
    BLOCKED_FILLS[why].fetch_add(1, Ordering::Relaxed);
    BLOCKED_FILLS[3].fetch_add(len as u64, Ordering::Relaxed);
}

/// Reads of ours of the title's memory on the card, by where they are
/// made: calls and bytes since the last [`say`].
static READS: Mutex<Option<HashMap<&'static Location<'static>, (u64, u64)>>> = Mutex::new(None);
static READ_TOTAL: [AtomicU64; 2] = [const { AtomicU64::new(0) }; 2];

/// A read of ours of `len` bytes at `at`: counted if it touches the
/// title's memory on the card.
pub(crate) fn read(at: usize, len: usize, from: &'static Location<'static>) {
    if HI.load(Ordering::Relaxed) == 0 || !touches(at, len) {
        return;
    }
    READ_TOTAL[0].fetch_add(1, Ordering::Relaxed);
    READ_TOTAL[1].fetch_add(len as u64, Ordering::Relaxed);
    let mut reads = READS.lock().unwrap_or_else(|e| e.into_inner());
    let e = reads.get_or_insert_with(HashMap::new).entry(from).or_default();
    e.0 += 1;
    e.1 += len as u64;
}

/// Once a second: the reads of the second before, biggest first.
pub(crate) fn say() {
    if HI.load(Ordering::Relaxed) == 0 {
        return;
    }
    let reads = READS.lock().unwrap_or_else(|e| e.into_inner()).take().unwrap_or_default();
    let mut reads: Vec<_> = reads.into_iter().collect();
    reads.sort_by_key(|r| std::cmp::Reverse(r.1.1));
    let text: Vec<String> = reads
        .iter()
        .take(8)
        .map(|(at, (n, bytes))| format!("{}:{} {n} reads {:.2} MB", at.file(), at.line(), *bytes as f64 / 1e6))
        .collect();
    let b = BLOCKED_FILLS.each_ref().map(|c| c.swap(0, Ordering::Relaxed));
    tracing::info!(
        "fills of upload memory that waited for the flights, the second before: {} with their source not on the card, {} partly on it, {} into memory outside the scratch; {:.2} MB",
        b[0],
        b[1],
        b[2],
        b[3] as f64 / 1e6
    );
    tracing::info!(
        "processor reads of the title's memory on the card, the second before: {} reads {:.2} MB ({}); since the start {} reads {:.1} MB",
        reads.iter().map(|r| r.1.0).sum::<u64>(),
        reads.iter().map(|r| r.1.1).sum::<u64>() as f64 / 1e6,
        if text.is_empty() { "none".to_owned() } else { text.join("; ") },
        READ_TOTAL[0].load(Ordering::Relaxed),
        READ_TOTAL[1].load(Ordering::Relaxed) as f64 / 1e6
    );
}
