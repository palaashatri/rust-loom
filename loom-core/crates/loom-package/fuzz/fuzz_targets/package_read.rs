#![no_main]

use libfuzzer_sys::fuzz_target;
use loom_package::PackageArchive;
use loom_package::zip::ArchiveLimits;

// Fuzz target for the Loom package container reader.
//
// The reader must never panic and must fail in a bounded way on truncated,
// malformed, pathologically sized, or checksum-corrupted archives. Entry
// count, per-entry size, and candidate offsets below keep the wrapper cheap;
// the size envelope is enforced again inside the reader's own limits.
fuzz_target!(|data: &[u8]| {
    let limits = ArchiveLimits {
        max_entries: 4096,
        max_entry_size: 64 * 1024 * 1024,
        max_archive_size: 1 << 30,
    };
    if let Ok(archive) = PackageArchive::from_bytes_with_limits(data, limits) {
        // Reading any single entry back must stay bounded as well.
        for path in archive.paths() {
            if let Ok(contents) = archive.get(&path) {
                let _ = contents.len();
            }
        }
    }
});