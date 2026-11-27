//! Hand-written shaders in place of translations (`VICTORY_NATIVE_SHADERS`).
//!
//! A console program whose translation costs too much on the card can be
//! written again by hand, in HLSL (`native\<hash>.hlsl` in the repository,
//! `native\README.md`), and compiled to DXIL (`target\native\<hash>.dxil`,
//! `tools\native-shaders.cmd`, which `build.bat` runs). With the switch on, a
//! program whose hash (the 16 digits a run's log names it by) has such a file
//! keeps everything its translation says of it — user data, samplers, root
//! views, the thread group the console gives it — and only its DXIL is taken
//! from the file. The hand-written shader must declare the same bindings as
//! the translation (`victory translate` with `VICTORY_EMIT_DUMP`, then
//! `dxc -dumpbin`): the root signature is made from the translation's.
//!
//! - unset or `0`: off; nothing is looked for.
//! - `1`: `target\native`, beside the folder of the running executable
//!   (`target\release`, `target\diag`).
//! - anything else: that folder.
//!
//! The shader cache keeps the translations as they are: the switch changes
//! nothing there, and off again needs no translating. Looked for once per
//! program, when its translation is first asked for; never per frame.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// The folder the switch names, or none when it is off.
fn folder() -> Option<&'static Path> {
    static FOLDER: OnceLock<Option<PathBuf>> = OnceLock::new();
    FOLDER
        .get_or_init(|| {
            let value = std::env::var_os("VICTORY_NATIVE_SHADERS")?;
            if value.is_empty() || value == "0" {
                return None;
            }
            let dir = if value == "1" {
                let exe = std::env::current_exe().ok()?;
                // <target>\<profile>\victory.exe -> <target>\native
                exe.parent()?.parent()?.join("native")
            } else {
                PathBuf::from(value)
            };
            let count = std::fs::read_dir(&dir)
                .map(|entries| {
                    entries
                        .filter_map(Result::ok)
                        .filter(|e| e.path().extension().is_some_and(|x| x == "dxil"))
                        .count()
                })
                .unwrap_or(0);
            tracing::info!(folder = %dir.display(), shaders = count, "hand-written shaders in place of translations (VICTORY_NATIVE_SHADERS)");
            Some(dir)
        })
        .as_deref()
}

/// The hash a run's log names a console program by: the first 16 hex digits
/// of the BLAKE3 of its container.
fn hash_of(container: &[u8]) -> String {
    blake3::hash(container).to_hex()[..16].to_owned()
}

/// The hand-written DXIL for `container`, when the switch is on and there is
/// one: read once per program and kept.
pub(crate) fn replacement(container: &[u8]) -> Option<Vec<u8>> {
    let dir = folder()?;
    static READ: OnceLock<Mutex<HashMap<String, Option<Vec<u8>>>>> = OnceLock::new();
    let hash = hash_of(container);
    let mut read = READ.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    read.entry(hash.clone())
        .or_insert_with(|| {
            let path = dir.join(format!("{hash}.dxil"));
            let bytes = std::fs::read(&path).ok()?;
            // A DXIL container begins as any shader container does.
            if !bytes.starts_with(b"DXBC") {
                tracing::warn!(path = %path.display(), "not a shader container; the translation is kept");
                return None;
            }
            tracing::info!(hash = %hash, bytes = bytes.len(), "hand-written shader in place of the translation");
            Some(bytes)
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::hash_of;

    /// The name is the log's: 16 lowercase hex digits of the container's BLAKE3.
    #[test]
    fn programs_are_named_as_the_log_names_them() {
        let h = hash_of(b"DXBC");
        assert_eq!(h.len(), 16);
        assert!(h.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
        assert_eq!(h, blake3::hash(b"DXBC").to_hex()[..16]);
    }
}
