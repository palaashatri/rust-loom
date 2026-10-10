//! Reads face descriptions from sfnt files (`.ttf`, `.otf`, `.ttc`, `.otc`).
//!
//! Scanning a system with thousands of fonts must not read thousands of
//! multi-megabyte files, so this reader asks only for the table directory and
//! the handful of small tables that describe a face: `head`, `OS/2`, `post`
//! (16 bytes) and `name`. Glyph data is never touched.

use crate::info::{
    normalize_family, EmbeddingPermission, FaceInfo, FaceSource, NamedInstance, VariationAxis,
    MAX_ALIASES,
};
use skrifa::raw::tables::name::Name;
use skrifa::raw::{FontData, FontRead};
use skrifa::string::StringId;
use std::fmt;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Largest `name` table accepted. Real tables are a few kilobytes; a few CJK
/// fonts carry several hundred.
const MAX_NAME_TABLE: u64 = 4 << 20;
/// Largest face count read from one collection file.
const MAX_COLLECTION_FONTS: u32 = 256;
/// Most table records accepted in one font's directory.
const MAX_TABLES: u16 = 1024;
/// Largest `fvar` table read (axes and instance records are tiny).
const MAX_FVAR_TABLE: usize = 1 << 20;
/// Most variation axes accepted (OpenType allows 64k; real fonts have a few).
const MAX_AXES: usize = 64;
/// Most named instances read from one font.
const MAX_INSTANCES: usize = 512;
/// Longest alias name kept, in characters.
const MAX_ALIAS_CHARS: usize = 128;
/// Most `name` records examined when collecting aliases.
const MAX_NAME_RECORDS_SCANNED: usize = 4096;

/// Why a file was not turned into a face.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SfntError {
    /// Not an sfnt (a WOFF, a Type 1 font, a text file, ...).
    NotAFont,
    /// A table or the directory points past the end of the file.
    Truncated,
    /// The font has no readable family or name.
    NoName,
    /// A read failed.
    Io(String),
}

impl fmt::Display for SfntError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAFont => f.write_str("not an OpenType/TrueType font"),
            Self::Truncated => f.write_str("font tables extend past the end of the file"),
            Self::NoName => f.write_str("font has no usable name table"),
            Self::Io(message) => write!(f, "read failed: {message}"),
        }
    }
}

impl std::error::Error for SfntError {}

/// Random access to the bytes of one font file.
pub(crate) trait Reader {
    fn len(&self) -> u64;
    fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>, SfntError>;
}

pub(crate) struct SliceReader<'a>(pub &'a [u8]);

impl Reader for SliceReader<'_> {
    fn len(&self) -> u64 {
        self.0.len() as u64
    }

    fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>, SfntError> {
        let start = usize::try_from(offset).map_err(|_| SfntError::Truncated)?;
        let end = start.checked_add(len).ok_or(SfntError::Truncated)?;
        self.0
            .get(start..end)
            .map(<[u8]>::to_vec)
            .ok_or(SfntError::Truncated)
    }
}

pub(crate) struct FileReader {
    file: File,
    len: u64,
}

impl FileReader {
    pub(crate) fn open(path: &Path) -> Result<Self, SfntError> {
        let file = File::open(path).map_err(|e| SfntError::Io(e.to_string()))?;
        let len = file
            .metadata()
            .map_err(|e| SfntError::Io(e.to_string()))?
            .len();
        Ok(Self { file, len })
    }
}

impl Reader for FileReader {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>, SfntError> {
        let end = offset.checked_add(len as u64).ok_or(SfntError::Truncated)?;
        if end > self.len {
            return Err(SfntError::Truncated);
        }
        let mut file = &self.file;
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| SfntError::Io(e.to_string()))?;
        let mut buffer = vec![0u8; len];
        file.read_exact(&mut buffer)
            .map_err(|e| SfntError::Io(e.to_string()))?;
        Ok(buffer)
    }
}

fn be_u16(buffer: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(buffer.get(at..at + 2)?.try_into().ok()?))
}

fn be_u32(buffer: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(buffer.get(at..at + 4)?.try_into().ok()?))
}

/// Reads every face in the font or collection behind `reader`.
///
/// One unreadable member of a collection is skipped; the whole file fails only
/// when no member can be read.
pub(crate) fn read_faces(
    reader: &dyn Reader,
    source: &FaceSource,
) -> Result<Vec<FaceInfo>, SfntError> {
    let header = reader.read_at(0, 12.min(reader.len() as usize))?;
    if header.len() < 12 {
        return Err(SfntError::NotAFont);
    }
    if &header[0..4] == b"ttcf" {
        let count = be_u32(&header, 8).ok_or(SfntError::NotAFont)?;
        if count == 0 || count > MAX_COLLECTION_FONTS {
            return Err(SfntError::NotAFont);
        }
        let offsets = reader.read_at(12, count as usize * 4)?;
        let mut faces = Vec::new();
        let mut last_error = SfntError::NotAFont;
        for (index, chunk) in offsets.chunks_exact(4).enumerate() {
            let base = u64::from(u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            match read_one(reader, base, source, index as u32) {
                Ok(face) => faces.push(face),
                Err(error) => last_error = error,
            }
        }
        if faces.is_empty() {
            Err(last_error)
        } else {
            Ok(faces)
        }
    } else {
        read_one(reader, 0, source, 0).map(|face| vec![face])
    }
}

struct Table {
    offset: u64,
    length: u64,
}

fn read_one(
    reader: &dyn Reader,
    base: u64,
    source: &FaceSource,
    index: u32,
) -> Result<FaceInfo, SfntError> {
    let header = reader.read_at(base, 12)?;
    // 0x00010000 TrueType, 'true' Apple TrueType, 'OTTO' CFF OpenType.
    match be_u32(&header, 0) {
        Some(0x0001_0000 | 0x7472_7565 | 0x4F54_544F) => {}
        _ => return Err(SfntError::NotAFont),
    }
    let table_count = be_u16(&header, 4).ok_or(SfntError::NotAFont)?;
    if table_count == 0 || table_count > MAX_TABLES {
        return Err(SfntError::NotAFont);
    }
    let directory = reader.read_at(base + 12, usize::from(table_count) * 16)?;
    let file_len = reader.len();
    let find = |tag: &[u8; 4]| -> Option<Table> {
        directory.chunks_exact(16).find_map(|record| {
            if &record[0..4] != tag {
                return None;
            }
            let offset = u64::from(be_u32(record, 8)?);
            let length = u64::from(be_u32(record, 12)?);
            (offset.checked_add(length)? <= file_len).then_some(Table { offset, length })
        })
    };
    let prefix = |tag: &[u8; 4], want: usize| -> Option<Vec<u8>> {
        let table = find(tag)?;
        let len = table.length.min(want as u64) as usize;
        reader.read_at(table.offset, len).ok()
    };

    let head = prefix(b"head", 54);
    let os2 = prefix(b"OS/2", 68);
    let post = prefix(b"post", 16);
    let variable = find(b"fvar").is_some();

    let name_table = find(b"name").ok_or(SfntError::NoName)?;
    if name_table.length > MAX_NAME_TABLE {
        return Err(SfntError::NoName);
    }
    let name_bytes = reader.read_at(name_table.offset, name_table.length as usize)?;
    let names = NameStrings::new(&name_bytes)?;

    let legacy = names.get(StringId::FAMILY_NAME);
    let typographic = names.get(StringId::TYPOGRAPHIC_FAMILY_NAME);
    let postscript = names.get(StringId::POSTSCRIPT_NAME).unwrap_or_default();
    let family = typographic
        .clone()
        .or_else(|| legacy.clone())
        .or_else(|| {
            postscript
                .split('-')
                .next()
                .filter(|part| !part.is_empty())
                .map(str::to_owned)
        })
        .ok_or(SfntError::NoName)?;
    let legacy_family = legacy.unwrap_or_else(|| family.clone());
    let aliases = family_aliases(&names, &[&family, &legacy_family]);

    let (axes, instances) = if variable {
        prefix(b"fvar", MAX_FVAR_TABLE)
            .and_then(|table| read_fvar(&table, &names))
            .unwrap_or_default()
    } else {
        (Vec::new(), Vec::new())
    };
    let style = names
        .get(StringId::TYPOGRAPHIC_SUBFAMILY_NAME)
        .or_else(|| names.get(StringId::SUBFAMILY_NAME))
        .unwrap_or_default();
    let full_name = names
        .get(StringId::FULL_NAME)
        .unwrap_or_else(|| format!("{family} {style}").trim().to_owned());

    let mac_style = head.as_deref().and_then(|h| be_u16(h, 44)).unwrap_or(0);
    let os2_weight = os2.as_deref().and_then(|t| be_u16(t, 4));
    let os2_width = os2.as_deref().and_then(|t| be_u16(t, 6));
    let fs_type = os2.as_deref().and_then(|t| be_u16(t, 8)).unwrap_or(0);
    let fs_selection = os2.as_deref().and_then(|t| be_u16(t, 62)).unwrap_or(0);
    let italic_angle = post.as_deref().and_then(|t| be_u32(t, 4)).unwrap_or(0);
    let fixed_pitch = post.as_deref().and_then(|t| be_u32(t, 12)).unwrap_or(0);

    let weight = match os2_weight {
        Some(w @ 1..=9) => w * 100,
        Some(w @ 10..=1000) => w,
        _ if mac_style & 1 != 0 => 700,
        _ => 400,
    };
    let width = os2_width.filter(|w| (1..=9).contains(w)).unwrap_or(5);
    let italic = fs_selection & 0x0201 != 0 || mac_style & 2 != 0 || italic_angle != 0;

    Ok(FaceInfo {
        family,
        legacy_family,
        aliases,
        style,
        full_name,
        postscript_name: postscript,
        weight,
        width,
        italic,
        monospace: fixed_pitch != 0,
        variable,
        axes,
        instances,
        embedding: EmbeddingPermission::from_fs_type(fs_type),
        subsetting_allowed: fs_type & 0x0100 == 0,
        source: source.clone(),
        index,
    })
}

/// Other-language and other-platform family names (name IDs 1 and 16), minus
/// the primary names and repeats, in `name` table order and bounded.
fn family_aliases(names: &NameStrings<'_>, primary: &[&str]) -> Vec<String> {
    let mut seen: Vec<String> = primary.iter().map(|n| normalize_family(n)).collect();
    let mut aliases = Vec::new();
    for id in [StringId::FAMILY_NAME, StringId::TYPOGRAPHIC_FAMILY_NAME] {
        for text in names.all(id) {
            if aliases.len() >= MAX_ALIASES {
                return aliases;
            }
            let key = normalize_family(&text);
            if text.chars().count() > MAX_ALIAS_CHARS || seen.contains(&key) {
                continue;
            }
            seen.push(key);
            aliases.push(text);
        }
    }
    aliases
}

fn fixed_to_f32(raw: u32) -> f32 {
    (raw as i32) as f32 / 65536.0
}

/// Parses an `fvar` table: axes and named instances. Returns `None` for a
/// table that is too short for what it declares.
fn read_fvar(
    table: &[u8],
    names: &NameStrings<'_>,
) -> Option<(Vec<VariationAxis>, Vec<NamedInstance>)> {
    let axes_offset = usize::from(be_u16(table, 4)?);
    let axis_count = usize::from(be_u16(table, 8)?);
    let axis_size = usize::from(be_u16(table, 10)?);
    let instance_count = usize::from(be_u16(table, 12)?);
    let instance_size = usize::from(be_u16(table, 14)?);
    if axis_count == 0 || axis_count > MAX_AXES || axis_size < 20 {
        return None;
    }
    let mut axes = Vec::with_capacity(axis_count);
    for index in 0..axis_count {
        let at = axes_offset + index * axis_size;
        let tag: [u8; 4] = table.get(at..at + 4)?.try_into().ok()?;
        axes.push(VariationAxis {
            tag,
            min: fixed_to_f32(be_u32(table, at + 4)?),
            default: fixed_to_f32(be_u32(table, at + 8)?),
            max: fixed_to_f32(be_u32(table, at + 12)?),
        });
    }
    let mut instances = Vec::new();
    let first = axes_offset + axis_count * axis_size;
    if instance_size >= 4 + axis_count * 4 {
        for index in 0..instance_count.min(MAX_INSTANCES) {
            let at = first + index * instance_size;
            let Some(name_id) = be_u16(table, at) else {
                break;
            };
            let coords: Option<Vec<f32>> = (0..axis_count)
                .map(|axis| be_u32(table, at + 4 + axis * 4).map(fixed_to_f32))
                .collect();
            let Some(coords) = coords else { break };
            instances.push(NamedInstance {
                style: names.get(StringId::new(name_id)).unwrap_or_default(),
                coords,
            });
        }
    }
    Some((axes, instances))
}

/// Name records of one font, picked by language preference.
struct NameStrings<'a> {
    table: Name<'a>,
}

impl<'a> NameStrings<'a> {
    fn new(bytes: &'a [u8]) -> Result<Self, SfntError> {
        Name::read(FontData::new(bytes))
            .map(|table| Self { table })
            .map_err(|_| SfntError::NoName)
    }

    /// Every non-empty string for `id`, in every language and platform, in
    /// table order. Reads at most [`MAX_NAME_RECORDS_SCANNED`] records.
    fn all(&self, id: StringId) -> Vec<String> {
        let data = self.table.string_data();
        self.table
            .name_record()
            .iter()
            .take(MAX_NAME_RECORDS_SCANNED)
            .filter(|record| record.name_id() == id)
            .filter_map(|record| record.string(data).ok())
            .map(|string| {
                let text: String = string.chars().filter(|c| *c != '\0').collect();
                text.trim().to_owned()
            })
            .filter(|text| !text.is_empty())
            .collect()
    }

    /// The best non-empty string for `id`: Windows English, then other
    /// English, Macintosh English, Unicode-platform, then anything.
    fn get(&self, id: StringId) -> Option<String> {
        let data = self.table.string_data();
        let mut best: Option<(u8, String)> = None;
        for record in self.table.name_record() {
            if record.name_id() != id {
                continue;
            }
            let rank = match (record.platform_id(), record.language_id()) {
                (3, 0x0409) => 0,
                (3, language) if language & 0xFF == 0x09 => 1,
                (1, 0) => 2,
                (0, _) => 3,
                (3, _) => 4,
                _ => 5,
            };
            if best.as_ref().is_some_and(|(current, _)| *current <= rank) {
                continue;
            }
            let Ok(string) = record.string(data) else {
                continue;
            };
            let text: String = string.chars().filter(|c| *c != '\0').collect();
            let text = text.trim();
            if !text.is_empty() {
                best = Some((rank, text.to_owned()));
            }
        }
        best.map(|(_, text)| text)
    }
}
