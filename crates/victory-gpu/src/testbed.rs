//! A title's view of the GPU outside a title: the device as the title
//! creates it (`ID3D12Device11`, so lists and queues have the newer
//! layouts), a direct queue, an allocator and a list, for tools that run
//! title code that records GPU work (`victory bvh-check`: the console's
//! ray-tracing structure builder on a test mesh). Submissions run and are
//! waited for as in a run with the default settings.
#![cfg(windows)]

use crate::object::{self, HRESULT, S_OK, This};

/// The objects a title would hold; leaked, as a title's are.
pub struct TitleGpu {
    pub device: This,
    pub queue: This,
    pub allocator: This,
    pub list: This,
    /// A shader-visible view heap, set on the list as a title sets its
    /// own: shaders that read title memory by address keep their views
    /// in it (`crate::memory`).
    pub heap: This,
}

/// Slots of the test heap.
const HEAP_SLOTS: u32 = 1 << 16;

// SAFETY: the objects are process-wide and thread-safe as for a title.
unsafe impl Send for TitleGpu {}

impl TitleGpu {
    /// The device (made with `create`, the runtime's device creation, so
    /// that it gets the title's allocator), then a direct queue, an
    /// allocator and a list in the recording state. `None` without a host
    /// GPU or if a creation fails. The list answers as the title's
    /// newest (the console's `List6`).
    pub fn new(
        create: extern "system" fn(*const std::ffi::c_void, *const u32, *const object::Iid, *mut This) -> HRESULT,
    ) -> Result<Self, String> {
        crate::host::host().ok_or("no host GPU")?;
        // Resource memory below 1 TiB, as the title's own allocator gives
        // (named before the runtime's device creation names the title's).
        crate::resource::use_allocator(low_alloc, low_free);
        let null: This = std::ptr::null();
        let version = crate::device::CHECKED_VERSION;
        let mut device = null;
        check(create(std::ptr::null(), &version, crate::device::newest_iid(), &mut device), "device")?;
        let desc = crate::queue::QueueDesc::direct();
        let mut queue = null;
        check(crate::queue::create(device, &desc, crate::queue::iid(), &mut queue), "queue")?;
        let mut allocator = null;
        check(crate::list::create_allocator(device, 0, crate::list::allocator_iid(), &mut allocator), "allocator")?;
        let mut list = null;
        check(crate::list::create_list(device, 0, 0, allocator, null, crate::list::list_iid(), &mut list), "list")?;
        // The plain description, then an empty placement (the heap makes
        // its own memory).
        let heap_desc = (crate::descriptor::HeapDesc::visible_views(HEAP_SLOTS), [0u64; 2]);
        let mut heap = null;
        check(
            crate::descriptor::create_heap(device, &heap_desc.0, crate::descriptor::heap_iid(), &mut heap),
            "descriptor heap",
        )?;
        let gpu = Self { device, queue, allocator, list, heap };
        gpu.set_heap();
        Ok(gpu)
    }

    /// `SetDescriptorHeaps` with the test heap (list slot 0x100).
    fn set_heap(&self) {
        let set = object::slot::<extern "system" fn(This, u32, *const This)>(self.list, 0x100);
        set(self.list, 1, &self.heap);
    }

    /// Closes the list, runs it on the queue (waited for), and makes it
    /// ready to record again.
    pub fn run(&self) -> Result<(), String> {
        let close = object::slot::<extern "system" fn(This) -> HRESULT>(self.list, 0x48);
        check(close(self.list), "list close")?;
        let execute = object::slot::<extern "system" fn(This, u32, *const This)>(self.queue, 0x50);
        execute(self.queue, 1, &self.list);
        // Settled: everything sent to the card is done and copied back.
        crate::host::settle_for_present();
        let reset = object::slot::<extern "system" fn(This, This, This) -> HRESULT>(self.list, 0x50);
        check(reset(self.list, self.allocator, std::ptr::null()), "list reset")?;
        self.set_heap();
        // What a frame boundary does for the work after it: pages shaders
        // found missing get their copies (`crate::memory`).
        crate::frame::begin();
        Ok(())
    }
}

/// A title buffer of `bytes` that shaders may write, made through the
/// device as a title makes one (the layer registers it and copies it as a
/// title buffer): its title memory. A ray-tracing structure and its scratch
/// memory live in such buffers. Never released.
pub fn title_buffer(gpu: &TitleGpu, bytes: u64) -> Result<*mut u8, String> {
    const UNORDERED_ACCESS: u32 = 8;
    let (heap, desc, iid) = crate::resource::uav_buffer_desc(bytes);
    let mut out: This = std::ptr::null();
    check(
        crate::resource::create_committed(gpu.device, &heap, 0, &desc, UNORDERED_ACCESS, std::ptr::null(), iid, &mut out),
        "title buffer",
    )?;
    let (data, _) = crate::resource::buffer_bytes(out).ok_or("title buffer without memory")?;
    if data as u64 >= 1 << 40 {
        return Err(format!("title buffer at {data:?}, past 40 bits"));
    }
    Ok(data)
}

/// Dispatches and draws recorded since the last call, and pipeline syncs
/// since the start.
pub fn counts() -> (u64, u64, u64) {
    let (draws, dispatches) = crate::list::take_counts();
    (dispatches, draws, crate::list::SYNCS.load(std::sync::atomic::Ordering::Relaxed))
}

/// `bytes` of zeroed read-write memory below 1 TiB, as title memory the GPU
/// reads by address must be (its addresses are the GPU's, 40 bits). Never
/// freed.
pub fn title_memory(bytes: usize) -> Result<*mut u8, String> {
    use windows_sys::Win32::System::Memory::{MEM_COMMIT, MEM_RESERVE, PAGE_READWRITE, VirtualAlloc};
    const LINE: usize = 64 << 10;
    let bytes = bytes.next_multiple_of(LINE);
    // From 64 GiB up, in steps of 4 GiB, the first free place.
    for n in 16..256usize {
        let at = n << 32;
        // SAFETY: a new mapping at a free address, or a refusal.
        let got = unsafe { VirtualAlloc(at as *const _, bytes, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE) };
        if !got.is_null() {
            return Ok(got.cast());
        }
    }
    Err(format!("no free place below 1 TiB for {bytes} bytes"))
}

extern "system" fn low_alloc(bytes: usize, _attributes: u64) -> *mut std::ffi::c_void {
    title_memory(bytes).map_or(std::ptr::null_mut(), |p| p.cast())
}
/// Kept: the test bed's memory lives as long as the process.
extern "system" fn low_free(_: *mut std::ffi::c_void, _attributes: u64) {}

fn check(hr: HRESULT, what: &str) -> Result<(), String> {
    if hr == S_OK { Ok(()) } else { Err(format!("{what}: {hr:#x}")) }
}
