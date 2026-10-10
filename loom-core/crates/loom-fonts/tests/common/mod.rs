//! Fixtures built from the bundled Inter faces only, so no test depends on
//! the fonts installed on the machine running it.

// Every test crate (and the `measure` example) compiles this module but uses
// only the helpers it needs; the rest would otherwise warn as dead code.
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

/// The tables of a plain (non-collection) sfnt, in directory order.
pub fn sfnt_tables(font: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
    let count = u16::from_be_bytes([font[4], font[5]]) as usize;
    (0..count)
        .map(|i| {
            let record = 12 + i * 16;
            let tag: [u8; 4] = font[record..record + 4].try_into().unwrap();
            let (offset, length) = (be32(font, record + 8), be32(font, record + 12));
            (tag, font[offset..offset + length].to_vec())
        })
        .collect()
}

/// Assembles a plain sfnt (TrueType outlines) from `tables`. Checksums are
/// left zero; the readers under test do not verify them.
pub fn build_sfnt(tables: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
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

/// `font` with table `tag` replaced by `data`, or added when absent.
pub fn with_table(font: &[u8], tag: &[u8; 4], data: Vec<u8>) -> Vec<u8> {
    let mut tables = sfnt_tables(font);
    match tables.iter_mut().find(|(t, _)| t == tag) {
        Some(slot) => slot.1 = data,
        None => tables.push((*tag, data)),
    }
    build_sfnt(&tables)
}

/// `font` with `bytes` written at `offset` inside table `tag`.
pub fn patch_table(font: &[u8], tag: &[u8; 4], offset: usize, bytes: &[u8]) -> Vec<u8> {
    let mut tables = sfnt_tables(font);
    let slot = tables
        .iter_mut()
        .find(|(t, _)| t == tag)
        .unwrap_or_else(|| panic!("no {} table", String::from_utf8_lossy(tag)));
    slot.1[offset..offset + bytes.len()].copy_from_slice(bytes);
    build_sfnt(&tables)
}

/// A format-0 `name` table. Each record is `(platform, language, name id,
/// text)`; platforms 0 and 3 are written as UTF-16BE, platform 1 as ASCII.
pub fn name_table(records: &[(u16, u16, u16, &str)]) -> Vec<u8> {
    let mut sorted = records.to_vec();
    sorted.sort_by_key(|(platform, language, id, _)| (*platform, *language, *id));
    let mut strings = Vec::new();
    let mut entries = Vec::new();
    for (platform, language, id, text) in &sorted {
        let encoded: Vec<u8> = if *platform == 1 {
            text.bytes().collect()
        } else {
            text.encode_utf16().flat_map(u16::to_be_bytes).collect()
        };
        let encoding: u16 = match platform {
            0 => 3,
            3 => 1,
            _ => 0,
        };
        let fields = [
            *platform,
            encoding,
            *language,
            *id,
            encoded.len() as u16,
            strings.len() as u16,
        ];
        entries.extend(fields.iter().flat_map(|f| f.to_be_bytes()));
        strings.extend_from_slice(&encoded);
    }
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&(sorted.len() as u16).to_be_bytes());
    out.extend_from_slice(&((6 + entries.len()) as u16).to_be_bytes());
    out.extend_from_slice(&entries);
    out.extend_from_slice(&strings);
    out
}

/// A format-4 `cmap` table mapping each `(code, glyph)` pair (ascending by
/// code, all in the BMP). A face carrying it covers exactly those characters,
/// which lets a test build a fallback font with coverage unlike Inter's.
pub fn cmap_table(mapping: &[(u16, u16)]) -> Vec<u8> {
    let segments = mapping.len() + 1;
    let selector = usize::BITS - 1 - segments.leading_zeros();
    let search_range = 2 * (1usize << selector);
    let mut ends: Vec<u16> = mapping.iter().map(|(code, _)| *code).collect();
    let mut starts = ends.clone();
    let mut deltas: Vec<u16> = mapping.iter().map(|(c, g)| g.wrapping_sub(*c)).collect();
    ends.push(0xFFFF);
    starts.push(0xFFFF);
    deltas.push(1);
    let mut sub = Vec::new();
    let length = 16 + segments * 8;
    for field in [
        4u16,
        length as u16,
        0,
        (segments * 2) as u16,
        search_range as u16,
        selector as u16,
        (segments * 2 - search_range) as u16,
    ] {
        sub.extend_from_slice(&field.to_be_bytes());
    }
    for end in &ends {
        sub.extend_from_slice(&end.to_be_bytes());
    }
    sub.extend_from_slice(&0u16.to_be_bytes());
    for start in &starts {
        sub.extend_from_slice(&start.to_be_bytes());
    }
    for delta in &deltas {
        sub.extend_from_slice(&delta.to_be_bytes());
    }
    sub.extend(std::iter::repeat(0u8).take(segments * 2));
    let mut out = Vec::new();
    for field in [0u16, 1, 3, 1] {
        out.extend_from_slice(&field.to_be_bytes());
    }
    out.extend_from_slice(&12u32.to_be_bytes());
    out.extend_from_slice(&sub);
    out
}

fn fixed(value: f32) -> [u8; 4] {
    ((value * 65536.0).round() as i32).to_be_bytes()
}

/// An `fvar` table: axes are `(tag, min, default, max)`, instances are
/// `(subfamily name id, one coordinate per axis)`.
pub fn fvar_table(axes: &[([u8; 4], f32, f32, f32)], instances: &[(u16, Vec<f32>)]) -> Vec<u8> {
    let instance_size = 4 + axes.len() * 4;
    let mut out = Vec::new();
    for field in [
        1u16,
        0,
        16,
        2,
        axes.len() as u16,
        20,
        instances.len() as u16,
    ] {
        out.extend_from_slice(&field.to_be_bytes());
    }
    out.extend_from_slice(&(instance_size as u16).to_be_bytes());
    for (tag, min, default, max) in axes {
        out.extend_from_slice(tag);
        for value in [min, default, max] {
            out.extend_from_slice(&fixed(*value));
        }
        out.extend_from_slice(&0u16.to_be_bytes()); // flags
        out.extend_from_slice(&256u16.to_be_bytes()); // axis name id
    }
    for (name_id, coords) in instances {
        out.extend_from_slice(&name_id.to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes());
        for coord in coords {
            out.extend_from_slice(&fixed(*coord));
        }
    }
    out
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
