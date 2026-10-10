//! Fixtures built from the bundled Inter faces only, so no test depends on
//! the fonts installed on the machine running it.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// A scratch directory removed on drop.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(tag: &str) -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "loom-fonts-{tag}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The bytes of a bundled Inter file such as `Inter-Regular.ttf`.
pub fn inter(file: &str) -> Vec<u8> {
    loom_fonts::bundled_faces()
        .iter()
        .find(|(name, _)| *name == file)
        .unwrap_or_else(|| panic!("{file} is not bundled"))
        .1
        .to_vec()
}

fn be32(bytes: &[u8], at: usize) -> usize {
    u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
}

fn replace_all(haystack: &mut [u8], needle: &[u8], with: &[u8]) {
    assert_eq!(needle.len(), with.len());
    let mut i = 0;
    while i + needle.len() <= haystack.len() {
        if &haystack[i..i + needle.len()] == needle {
            haystack[i..i + needle.len()].copy_from_slice(with);
            i += needle.len();
        } else {
            i += 1;
        }
    }
}

/// A copy of an Inter face whose name table says `family` (exactly five
/// ASCII characters) instead of "Inter", so it is a distinct family.
pub fn rename_family(font: &[u8], family: &str) -> Vec<u8> {
    assert_eq!(family.len(), 5, "same length keeps the table layout");
    let mut out = font.to_vec();
    let tables = u16::from_be_bytes([out[4], out[5]]) as usize;
    let mut name = None;
    for i in 0..tables {
        let record = 12 + i * 16;
        if &out[record..record + 4] == b"name" {
            name = Some((be32(&out, record + 8), be32(&out, record + 12)));
        }
    }
    let (offset, length) = name.expect("name table");
    let table = &mut out[offset..offset + length];
    replace_all(table, b"Inter", family.as_bytes());
    let utf16 = |s: &str| -> Vec<u8> { s.bytes().flat_map(|b| [0, b]).collect() };
    replace_all(table, &utf16("Inter"), &utf16(family));
    out
}

/// A `.ttc` holding `fonts` (each a plain sfnt).
pub fn make_collection(fonts: &[Vec<u8>]) -> Vec<u8> {
    let header = 12 + 4 * fonts.len();
    let mut out = Vec::new();
    out.extend_from_slice(b"ttcf");
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    out.extend_from_slice(&(fonts.len() as u32).to_be_bytes());
    let mut bases = Vec::new();
    let mut next = header;
    for font in fonts {
        bases.push(next);
        out.extend_from_slice(&(next as u32).to_be_bytes());
        next += font.len();
    }
    for (font, base) in fonts.iter().zip(bases) {
        let mut copy = font.clone();
        let tables = u16::from_be_bytes([copy[4], copy[5]]) as usize;
        for i in 0..tables {
            let at = 12 + i * 16 + 8;
            let shifted = be32(&copy, at) + base;
            copy[at..at + 4].copy_from_slice(&(shifted as u32).to_be_bytes());
        }
        out.extend_from_slice(&copy);
    }
    out
}

/// Writes `count` distinct fonts (renamed Inter faces, `F0000` upward) into
/// `dir` and returns their paths. Six faces per family.
pub fn write_family_variants(dir: &Path, count: usize) -> Vec<PathBuf> {
    let files = [
        "Inter-Regular.ttf",
        "Inter-Italic.ttf",
        "Inter-Medium.ttf",
        "Inter-SemiBold.ttf",
        "Inter-Bold.ttf",
        "Inter-BoldItalic.ttf",
    ];
    let originals: Vec<Vec<u8>> = files.iter().map(|f| inter(f)).collect();
    let mut paths = Vec::new();
    for n in 0..count {
        let family = format!("F{:04}", n / files.len());
        let source = &originals[n % files.len()];
        let bytes = rename_family(source, &family);
        let path = dir.join(format!("{family}-{}-{n:04}.ttf", n % files.len()));
        fs::write(&path, bytes).unwrap();
        paths.push(path);
    }
    paths
}

/// A tiny deterministic generator for property tests (no extra crate).
pub struct Lcg(pub u64);

impl Lcg {
    pub fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    /// A string of `len` characters from `alphabet`.
    pub fn string(&mut self, alphabet: &[char], len: usize) -> String {
        (0..len)
            .map(|_| alphabet[self.below(alphabet.len() as u64) as usize])
            .collect()
    }
}
