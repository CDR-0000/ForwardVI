//! What the title said about its memory when it asked for it (profile
//! builds): the console's protection bits (GPU access, write-combining,
//! uncached) of a reservation and the memory type of an `XMemAlloc`. The
//! host drops them; the frame trace names every copy refresh by them, to
//! see which kinds of memory the copies of title memory cost most.
//! Without the `profile` feature nothing is kept.
//!
//! What it found (ForwardVI, 26 November): write-combined memory
//! (`0x2404`, one 2.2 GB reservation) took 80 ms a frame of copy refreshes,
//! 3,450 small buffers for 50 MB; the physical page pool's views (`0x801`)
//! 68 ms. Reading the small write-combined buffers in place (the GPU on
//! the title's own memory, as a PC title's upload heap) left typical
//! submissions faster but some took 100-345 ms and the title itself
//! 70-85 ms a frame slower, as one heap per buffer, one for the whole
//! reservation or 64 MB pieces alike: frames went from 221-247 ms to
//! 326-368. Not kept.

/// The title asked for `[at, at + len)` with `flags` (raw, as passed)
/// through `source` (`"virtual"`: `XMemVirtualAlloc`'s protection;
/// `"alloc"`: `XMemAlloc`'s memory type). A later range at the same start
/// takes its place; one inside another is found first.
#[cfg_attr(not(feature = "profile"), allow(unused_variables))]
pub fn title_memory_kind(at: usize, len: usize, source: &'static str, flags: u64) {
    #[cfg(feature = "profile")]
    kept::put(at, len, source, flags);
}

/// The title freed what [`title_memory_kind`] noted at `at`.
#[cfg_attr(not(feature = "profile"), allow(unused_variables))]
pub fn title_memory_kind_gone(at: usize) {
    #[cfg(feature = "profile")]
    kept::gone(at);
}

/// The name of the kind of memory at `at`: `"virtual p=0x..."`,
/// `"alloc t=..."`, or `"unknown"`; `"-"` unless `VICTORY_PROFILE_MEMORY=1`
/// asks for it (a lookup under a lock for every copy refresh, some 3,500 a
/// frame, kept out of the frame times otherwise).
#[cfg(feature = "profile")]
pub(crate) fn of(at: usize) -> &'static str {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if !*ON.get_or_init(|| std::env::var_os("VICTORY_PROFILE_MEMORY").is_some_and(|v| v == "1")) {
        return "-";
    }
    kept::of(at)
}

#[cfg(feature = "profile")]
mod kept {
    use std::collections::{BTreeMap, HashMap};
    use std::sync::{Mutex, RwLock};

    /// By start: end and name.
    static RANGES: RwLock<BTreeMap<usize, (usize, &'static str)>> = RwLock::new(BTreeMap::new());
    /// Names made so far, by source and flags (a few dozen at most).
    static NAMES: Mutex<Option<HashMap<(&'static str, u64), &'static str>>> = Mutex::new(None);

    fn name(source: &'static str, flags: u64) -> &'static str {
        let mut names = NAMES.lock().unwrap_or_else(|e| e.into_inner());
        let names = names.get_or_insert_with(HashMap::new);
        names.entry((source, flags)).or_insert_with(|| {
            let name = match source {
                "virtual" => format!("virtual p={flags:#x}"),
                "alloc" => format!("alloc t={flags:#x}"),
                _ => format!("{source} {flags:#x}"),
            };
            tracing::info!(memory = %name, "a kind of title memory first asked for");
            Box::leak(name.into_boxed_str())
        })
    }

    pub(super) fn put(at: usize, len: usize, source: &'static str, flags: u64) {
        if at == 0 || len == 0 {
            return;
        }
        let name = name(source, flags);
        RANGES.write().unwrap_or_else(|e| e.into_inner()).insert(at, (at.saturating_add(len), name));
    }

    pub(super) fn gone(at: usize) {
        RANGES.write().unwrap_or_else(|e| e.into_inner()).remove(&at);
    }

    pub(super) fn of(at: usize) -> &'static str {
        let ranges = RANGES.read().unwrap_or_else(|e| e.into_inner());
        // The nearest start below that holds it: an allocation inside a
        // reservation starts later, so it is met first.
        // shortcut: only the 64 nearest starts are looked at.
        ranges.range(..=at).rev().take(64).find(|(_, range)| at < range.0).map_or("unknown", |(_, range)| range.1)
    }
}
