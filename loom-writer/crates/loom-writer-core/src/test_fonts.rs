//! Font fixtures for tests, built from the bundled Inter faces only, so no test
//! depends on what the machine running it has installed.
//!
//! A fixture family is a copy of Inter's four document faces (Regular, Italic,
//! Bold, Bold Italic) with its `name` table rewritten, and optionally with
//! every advance width scaled, so a test can tell "measured in the family" from
//! "measured in Inter" by a wide margin. Installing a fixture replaces the
//! process-wide catalogue; fixtures serialise on a lock and restore the
//! bundled-only catalogue when dropped.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::fonts::{install_font_catalog, reset_font_catalog, FontCatalog, ScanConfig};

static LOCK: Mutex<()> = Mutex::new(());

/// File, subfamily name and OS/2 style of the four faces a document uses.
const FACES: [(&str, &str); 4] = [
    ("Inter-Regular.ttf", "Regular"),
    ("Inter-Italic.ttf", "Italic"),
    ("Inter-Bold.ttf", "Bold"),
    ("Inter-BoldItalic.ttf", "Bold Italic"),
];

/// Holds the fixture lock without installing anything. A test that counts
/// layout cache misses takes it so that no other test replaces the font
/// catalogue (which invalidates every cache) while it counts.
pub fn serial() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The installed fixture catalogue. Dropping it restores the bundled-only
/// catalogue and removes the files.
pub struct FixtureFonts {
    dir: PathBuf,
    _lock: MutexGuard<'static, ()>,
}

/// Installs the fixture families `(name, advance percent)` as the process
/// catalogue. A percent of 100 keeps Inter's advances; 200 doubles them.
pub fn install(families: &[(&str, u32)]) -> FixtureFonts {
    let lock = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "loom-writer-fixture-fonts-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).expect("fixture font directory");
    for (family, percent) in families {
        write_family(&dir, family, *percent);
    }
    let fixture = FixtureFonts { dir, _lock: lock };
    install_font_catalog(std::sync::Arc::new(FontCatalog::scan(
        &fixture.scan_config(),
    )));
    fixture
}

impl FixtureFonts {
    /// Adds `families` to the directory and installs a catalogue that has
    /// them, as when a background scan finishes after the window opened.
    pub fn add(&self, families: &[(&str, u32)]) {
        for (family, percent) in families {
            write_family(&self.dir, family, *percent);
        }
        install_font_catalog(std::sync::Arc::new(FontCatalog::scan(&self.scan_config())));
    }

    /// The directory the font files are in.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// A scan configuration that reads this fixture directory (and the
    /// bundled faces), for tests of the scanner itself.
    pub fn scan_config(&self) -> ScanConfig {
        ScanConfig::with_dirs(vec![self.dir.clone()])
    }

    /// The font files written for `family`, regular first.
    pub fn files(&self, family: &str) -> Vec<PathBuf> {
        FACES
            .iter()
            .map(|(_, style)| self.dir.join(file_name(family, style)))
            .collect()
    }
}

impl Drop for FixtureFonts {
    fn drop(&mut self) {
        reset_font_catalog();
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn file_name(family: &str, style: &str) -> String {
    format!("{}-{}.ttf", family.replace(' ', ""), style.replace(' ', ""))
}

fn write_family(dir: &Path, family: &str, percent: u32) {
    for (source, style) in FACES {
        let bytes = loom_fonts::bundled_faces()
            .iter()
            .find(|(name, _)| *name == source)
            .map(|(_, bytes)| bytes.to_vec())
            .unwrap_or_else(|| panic!("{source} is not bundled"));
        let full = format!("{family} {style}");
        let postscript = format!("{}-{}", family.replace(' ', ""), style.replace(' ', ""));
        let mut tables = sfnt_tables(&bytes);
        for (tag, data) in &mut tables {
            if tag == b"name" {
                *data = name_table(&[
                    (1, family),
                    (2, style),
                    (4, full.as_str()),
                    (6, postscript.as_str()),
                ]);
            }
        }
        if percent != 100 {
            scale_advances(&mut tables, percent);
        }
        fs::write(dir.join(file_name(family, style)), build_sfnt(&tables)).expect("fixture font");
    }
}

fn be16(bytes: &[u8], at: usize) -> usize {
    usize::from(u16::from_be_bytes([bytes[at], bytes[at + 1]]))
}

fn be32(bytes: &[u8], at: usize) -> usize {
    u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]) as usize
}

fn sfnt_tables(font: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
    (0..be16(font, 4))
        .map(|index| {
            let record = 12 + index * 16;
            let tag = [
                font[record],
                font[record + 1],
                font[record + 2],
                font[record + 3],
            ];
            let (offset, length) = (be32(font, record + 8), be32(font, record + 12));
            (tag, font[offset..offset + length].to_vec())
        })
        .collect()
}

fn build_sfnt(tables: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut sorted: Vec<&([u8; 4], Vec<u8>)> = tables.iter().collect();
    sorted.sort_by_key(|(tag, _)| *tag);
    let count = sorted.len();
    let selector = usize::BITS - 1 - count.leading_zeros();
    let search_range = 16 * (1usize << selector);
    let mut out = Vec::new();
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    out.extend_from_slice(&(count as u16).to_be_bytes());
    out.extend_from_slice(&(search_range as u16).to_be_bytes());
    out.extend_from_slice(&(selector as u16).to_be_bytes());
    out.extend_from_slice(&((count * 16 - search_range) as u16).to_be_bytes());
    let mut offset = 12 + count * 16;
    for (tag, data) in &sorted {
        out.extend_from_slice(tag);
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&(offset as u32).to_be_bytes());
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        offset += data.len().div_ceil(4) * 4;
    }
    for (_, data) in &sorted {
        out.extend_from_slice(data);
        out.resize(out.len().div_ceil(4) * 4, 0);
    }
    out
}

/// A format-0 `name` table with Windows (platform 3) Unicode records.
fn name_table(records: &[(u16, &str)]) -> Vec<u8> {
    let mut strings = Vec::new();
    let mut entries = Vec::new();
    for (id, text) in records {
        let encoded: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
        for field in [
            3u16,
            1,
            0x409,
            *id,
            encoded.len() as u16,
            strings.len() as u16,
        ] {
            entries.extend_from_slice(&field.to_be_bytes());
        }
        strings.extend_from_slice(&encoded);
    }
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&(records.len() as u16).to_be_bytes());
    out.extend_from_slice(&((6 + entries.len()) as u16).to_be_bytes());
    out.extend_from_slice(&entries);
    out.extend_from_slice(&strings);
    out
}

/// Scales every advance in `hmtx` by `percent` / 100.
fn scale_advances(tables: &mut [([u8; 4], Vec<u8>)], percent: u32) {
    let metrics = tables
        .iter()
        .find(|(tag, _)| tag == b"hhea")
        .map(|(_, data)| be16(data, 34))
        .expect("hhea table");
    let hmtx = tables
        .iter_mut()
        .find(|(tag, _)| tag == b"hmtx")
        .map(|(_, data)| data)
        .expect("hmtx table");
    for index in 0..metrics {
        let at = index * 4;
        let scaled = (be16(hmtx, at) as u32 * percent / 100).min(u32::from(u16::MAX)) as u16;
        hmtx[at..at + 2].copy_from_slice(&scaled.to_be_bytes());
    }
}
