//! A minimal, dependency-free ZIP reader/writer for Loom packages.
//!
//! Entries are stored uncompressed (method 0) in this foundational version;
//! a compression feature is planned behind `.cargo/config.toml` feature flags.
//! We implement CRC-32 and SHA-256 ourselves so the crate has zero mandatory
//! dependencies. Safety limits guard against archive bombs and malformed
//! content, and paths are normalized to prevent traversal.

use crate::manifest::{Checksum, ManifestError};
use std::collections::BTreeMap;
use std::fmt;

/// Maximum number of entries permitted in an archive.
pub const MAX_ENTRIES: usize = 4096;
/// Maximum uncompressed size per entry.
pub const MAX_ENTRY_SIZE: u64 = 512 * 1024 * 1024;
/// Maximum total uncompressed size.
pub const MAX_TOTAL_SIZE: u64 = 8 * 1024 * 1024 * 1024;
/// Maximum path length in bytes.
pub const MAX_PATH_LEN: usize = 1024;

/// Archive-specific limits (overridable).
#[derive(Debug, Clone, Copy)]
pub struct ArchiveLimits {
    /// Maximum number of entries.
    pub max_entries: usize,
    /// Maximum uncompressed bytes per entry.
    pub max_entry_size: u64,
    /// Maximum total uncompressed bytes.
    pub max_total_size: u64,
    /// Maximum path byte length.
    pub max_path_len: usize,
}

impl Default for ArchiveLimits {
    fn default() -> Self {
        Self {
            max_entries: MAX_ENTRIES,
            max_entry_size: MAX_ENTRY_SIZE,
            max_total_size: MAX_TOTAL_SIZE,
            max_path_len: MAX_PATH_LEN,
        }
    }
}

/// Errors from ZIP reading/writing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchiveError {
    /// Generic I/O problem.
    Io(String),
    /// Signature mismatch at an offset.
    BadSignature {
        /// Where it failed.
        expected: &'static str,
        /// Offset.
        offset: u64,
    },
    /// Truncated file.
    Truncated,
    /// Unsupported compression method.
    UnsupportedMethod(u16),
    /// Unsupported ZIP version required.
    UnsupportedVersion(u16),
    /// Entry count exceeds limit.
    TooManyEntries(usize),
    /// Entry exceeds size limit.
    EntryTooLarge(String, u64),
    /// Total size exceeds limit.
    TotalTooLarge(u64),
    /// Path is unsafe (traversal or absolute).
    UnsafePath(String),
    /// Path too long.
    PathTooLong(String),
    /// Duplicate entry.
    DuplicateEntry(String),
    /// Checksum mismatch.
    ChecksumMismatch(String),
    /// Corrupt data (bad CRC, missing entry, etc.).
    Corrupt(String),
    /// Manifest error.
    Manifest(ManifestError),
    /// Invalid argument.
    InvalidArgument(String),
}

impl fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io: {e}"),
            Self::BadSignature { expected, offset } => {
                write!(f, "bad signature for {expected} at offset {offset}")
            }
            Self::Truncated => write!(f, "truncated archive"),
            Self::UnsupportedMethod(m) => write!(f, "unsupported compression method {m}"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported zip version {v}"),
            Self::TooManyEntries(n) => write!(f, "too many entries: {n}"),
            Self::EntryTooLarge(p, n) => write!(f, "entry {p} too large: {n}"),
            Self::TotalTooLarge(n) => write!(f, "total size too large: {n}"),
            Self::UnsafePath(p) => write!(f, "unsafe path: {p}"),
            Self::PathTooLong(p) => write!(f, "path too long: {p}"),
            Self::DuplicateEntry(p) => write!(f, "duplicate entry: {p}"),
            Self::ChecksumMismatch(p) => write!(f, "checksum mismatch: {p}"),
            Self::Corrupt(e) => write!(f, "corrupt archive: {e}"),
            Self::Manifest(e) => write!(f, "manifest: {e}"),
            Self::InvalidArgument(e) => write!(f, "invalid argument: {e}"),
        }
    }
}

impl core::error::Error for ArchiveError {}

impl From<ManifestError> for ArchiveError {
    fn from(e: ManifestError) -> Self {
        Self::Manifest(e)
    }
}

/// A single archived entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Normalized path within the archive.
    pub path: String,
    /// Uncompressed content bytes.
    pub data: Vec<u8>,
}

/// CRC-32 (IEEE, reflected) implementation.
pub struct Crc32 {
    table: [u32; 256],
}

impl Default for Crc32 {
    fn default() -> Self {
        Self::new()
    }
}

impl Crc32 {
    /// Create a new CRC-32 table.
    pub fn new() -> Self {
        let mut table = [0u32; 256];
        for i in 0..256u32 {
            let mut c = i;
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
            table[i as usize] = c;
        }
        Self { table }
    }

    /// Compute CRC-32 over bytes.
    pub fn checksum(&self, data: &[u8]) -> u32 {
        let mut c = 0xFFFF_FFFFu32;
        for &b in data {
            c = self.table[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
        }
        c ^ 0xFFFF_FFFF
    }
}

/// SHA-256 implementation.
pub struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    buf_len: usize,
    total_len: u64,
}

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha256 {
    /// Create a new SHA-256 hasher.
    pub fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buffer: [0u8; 64],
            buf_len: 0,
            total_len: 0,
        }
    }

    /// Feed bytes.
    pub fn update(&mut self, mut data: &[u8]) {
        self.total_len = self.total_len.wrapping_add(data.len() as u64);
        if self.buf_len > 0 {
            let need = 64 - self.buf_len;
            let take = need.min(data.len());
            self.buffer[self.buf_len..self.buf_len + take].copy_from_slice(&data[..take]);
            self.buf_len += take;
            data = &data[take..];
            if self.buf_len == 64 {
                let block = self.buffer;
                self.compress(&block);
                self.buf_len = 0;
            }
        }
        while data.len() >= 64 {
            let block: [u8; 64] = data[..64].try_into().unwrap();
            self.compress(&block);
            data = &data[64..];
        }
        if !data.is_empty() {
            self.buffer[..data.len()].copy_from_slice(data);
            self.buf_len = data.len();
        }
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut a = self.state[0];
        let mut b = self.state[1];
        let mut c = self.state[2];
        let mut d = self.state[3];
        let mut e = self.state[4];
        let mut f = self.state[5];
        let mut g = self.state[6];
        let mut h = self.state[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }

    /// Finalize into a digest.
    pub fn finalize(mut self) -> [u8; 32] {
        let bit_len = self.total_len.wrapping_mul(8);
        self.update(&[0x80]);
        while self.buf_len != 56 {
            self.update(&[0x00]);
        }
        let mut len_bytes = [0u8; 8];
        len_bytes.copy_from_slice(&bit_len.to_be_bytes());
        self.update(&len_bytes);

        let mut out = [0u8; 32];
        for (i, v) in self.state.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
        }
        out
    }
}

/// Compute SHA-256 of bytes.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize()
}

/// Normalize an archive path: strip leading `/`, collapse `.`/`..`,
/// forbid traversal above root, forbid NUL.
pub fn normalize_path(p: &str) -> Result<String, ArchiveError> {
    if p.is_empty() {
        return Err(ArchiveError::InvalidArgument("empty path".into()));
    }
    if p.contains('\0') {
        return Err(ArchiveError::UnsafePath(p.to_string()));
    }
    if p.starts_with('/') {
        return Err(ArchiveError::UnsafePath(p.to_string()));
    }
    if p.contains("\\") {
        return Err(ArchiveError::UnsafePath(p.to_string()));
    }
    let parts: Vec<&str> = p.split('/').collect();
    let mut stack: Vec<&str> = Vec::new();
    for part in parts {
        match part {
            "" | "." => {}
            ".." => {
                if stack.is_empty() {
                    return Err(ArchiveError::UnsafePath(p.to_string()));
                }
                stack.pop();
            }
            _ => stack.push(part),
        }
    }
    if stack.is_empty() {
        return Err(ArchiveError::InvalidArgument("empty path".into()));
    }
    Ok(stack.join("/"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LocalHeader {
    method: u16,
    crc: u32,
    compressed_size: u32,
    uncompressed_size: u32,
    name_len: u16,
    extra_len: u16,
}

/// A parsed ZIP archive in memory.
#[derive(Debug, Clone)]
pub struct PackageArchive {
    entries: BTreeMap<String, Vec<u8>>,
    limits: ArchiveLimits,
}

impl PackageArchive {
    /// Create an empty archive with default limits.
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            limits: ArchiveLimits::default(),
        }
    }

    /// Create an empty archive with custom limits.
    pub fn with_limits(limits: ArchiveLimits) -> Self {
        Self {
            entries: BTreeMap::new(),
            limits,
        }
    }

    /// Add an entry, normalizing and validating path and size limits.
    pub fn add(&mut self, path: &str, data: Vec<u8>) -> Result<(), ArchiveError> {
        let path = normalize_path(path)?;
        if path.len() > self.limits.max_path_len {
            return Err(ArchiveError::PathTooLong(path));
        }
        if data.len() as u64 > self.limits.max_entry_size {
            return Err(ArchiveError::EntryTooLarge(path, data.len() as u64));
        }
        if self.entries.contains_key(&path) {
            return Err(ArchiveError::DuplicateEntry(path));
        }
        if self.entries.len() >= self.limits.max_entries {
            return Err(ArchiveError::TooManyEntries(self.entries.len() + 1));
        }
        let total: u64 =
            self.entries.values().map(|v| v.len() as u64).sum::<u64>() + data.len() as u64;
        if total > self.limits.max_total_size {
            return Err(ArchiveError::TotalTooLarge(total));
        }
        self.entries.insert(path, data);
        Ok(())
    }

    /// Get a single entry.
    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.entries.get(path).map(|v| v.as_slice())
    }

    /// List entry paths in lexicographic order.
    pub fn paths(&self) -> Vec<&str> {
        self.entries.keys().map(|s| s.as_str()).collect()
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Serialize to a ZIP file image (stored method).
    pub fn to_bytes(&self) -> Result<Vec<u8>, ArchiveError> {
        let crc = Crc32::new();
        let mut out = Vec::new();
        let mut offsets: Vec<(String, u32, LocalHeader)> = Vec::with_capacity(self.entries.len());
        for (path, data) in &self.entries {
            if path.len() > self.limits.max_path_len {
                return Err(ArchiveError::PathTooLong(path.clone()));
            }
            let offset = out.len() as u32;
            // Local file header.
            out.extend_from_slice(&0x04034b50u32.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes()); // version needed
            out.extend_from_slice(&0u16.to_le_bytes()); // flags
            out.extend_from_slice(&0u16.to_le_bytes()); // method: stored
            out.extend_from_slice(&0u16.to_le_bytes()); // time
            out.extend_from_slice(&0u16.to_le_bytes()); // date
            let crc_val = crc.checksum(data);
            out.extend_from_slice(&crc_val.to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(path.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // extra len
            out.extend_from_slice(path.as_bytes());
            out.extend_from_slice(data);
            offsets.push((
                path.clone(),
                offset,
                LocalHeader {
                    method: 0,
                    crc: crc_val,
                    compressed_size: data.len() as u32,
                    uncompressed_size: data.len() as u32,
                    name_len: path.len() as u16,
                    extra_len: 0,
                },
            ));
        }
        let cd_start = out.len() as u32;
        for (path, offset, hdr) in &offsets {
            out.extend_from_slice(&0x02014b50u32.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes()); // version made by
            out.extend_from_slice(&20u16.to_le_bytes()); // version needed
            out.extend_from_slice(&0u16.to_le_bytes()); // flags
            out.extend_from_slice(&hdr.method.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // time
            out.extend_from_slice(&0u16.to_le_bytes()); // date
            out.extend_from_slice(&hdr.crc.to_le_bytes());
            out.extend_from_slice(&hdr.compressed_size.to_le_bytes());
            out.extend_from_slice(&hdr.uncompressed_size.to_le_bytes());
            out.extend_from_slice(&(path.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // extra len
            out.extend_from_slice(&0u16.to_le_bytes()); // comment len
            out.extend_from_slice(&0u16.to_le_bytes()); // disk start
            out.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
            out.extend_from_slice(&0u32.to_le_bytes()); // external attrs
            out.extend_from_slice(&offset.to_le_bytes());
            out.extend_from_slice(path.as_bytes());
        }
        let cd_size = out.len() as u32 - cd_start;
        let cd_count = offsets.len() as u16;
        // End of central directory.
        out.extend_from_slice(&0x06054b50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // disk
        out.extend_from_slice(&0u16.to_le_bytes()); // cd disk
        out.extend_from_slice(&cd_count.to_le_bytes());
        out.extend_from_slice(&cd_count.to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_start.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // comment len
        Ok(out)
    }

    /// Parse a ZIP file image.
    pub fn from_bytes(data: &[u8]) -> Result<Self, ArchiveError> {
        Self::from_bytes_with_limits(data, ArchiveLimits::default())
    }

    /// Parse a ZIP file image with custom limits.
    pub fn from_bytes_with_limits(
        data: &[u8],
        limits: ArchiveLimits,
    ) -> Result<Self, ArchiveError> {
        // Locate End of Central Directory.
        let mut eocd_pos = None;
        // Search backwards for signature within the last 65557 bytes.
        let search_start = data.len().saturating_sub(65557);
        for i in (search_start..data.len().saturating_sub(21)).rev() {
            if data[i..i + 4] == [0x50, 0x4b, 0x05, 0x06] {
                eocd_pos = Some(i);
                break;
            }
        }
        let eocd_pos = eocd_pos.ok_or(ArchiveError::Truncated)?;
        if eocd_pos + 22 > data.len() {
            return Err(ArchiveError::Truncated);
        }
        let disk = u16::from_le_bytes([data[eocd_pos + 4], data[eocd_pos + 5]]);
        let cd_disk = u16::from_le_bytes([data[eocd_pos + 6], data[eocd_pos + 7]]);
        let this_disk_entries = u16::from_le_bytes([data[eocd_pos + 8], data[eocd_pos + 9]]);
        let total_entries = u16::from_le_bytes([data[eocd_pos + 10], data[eocd_pos + 11]]);
        let cd_size = u32::from_le_bytes([
            data[eocd_pos + 12],
            data[eocd_pos + 13],
            data[eocd_pos + 14],
            data[eocd_pos + 15],
        ]);
        let cd_offset = u32::from_le_bytes([
            data[eocd_pos + 16],
            data[eocd_pos + 17],
            data[eocd_pos + 18],
            data[eocd_pos + 19],
        ]);
        let comment_len = u16::from_le_bytes([data[eocd_pos + 20], data[eocd_pos + 21]]);
        if comment_len as usize > data.len().saturating_sub(eocd_pos + 22) {
            return Err(ArchiveError::Truncated);
        }
        if disk != 0 || cd_disk != 0 {
            return Err(ArchiveError::UnsupportedVersion(0));
        }
        if this_disk_entries != total_entries {
            return Err(ArchiveError::Corrupt(
                "multi-disk or mismatched entry counts unsupported".into(),
            ));
        }
        if total_entries as usize > limits.max_entries {
            return Err(ArchiveError::TooManyEntries(total_entries as usize));
        }
        if cd_offset as usize > data.len()
            || cd_size as usize > data.len().saturating_sub(cd_offset as usize)
        {
            return Err(ArchiveError::Truncated);
        }
        let cd_end = cd_offset as usize + cd_size as usize;
        if cd_end > data.len() {
            return Err(ArchiveError::Truncated);
        }

        let mut parser = Cursor {
            data,
            pos: cd_offset as usize,
        };
        let mut entries = BTreeMap::new();
        let mut total_size: u64 = 0;
        let crc = Crc32::new();
        for _ in 0..total_entries {
            if parser.remaining() < 46 {
                return Err(ArchiveError::Truncated);
            }
            let sig = parser.read_u32()?;
            if sig != 0x02014b50 {
                return Err(ArchiveError::BadSignature {
                    expected: "central directory",
                    offset: parser.pos as u64 - 4,
                });
            }
            let _made_by = parser.read_u16()?;
            let needed = parser.read_u16()?;
            let _flags = parser.read_u16()?;
            let method = parser.read_u16()?;
            let _time = parser.read_u16()?;
            let _date = parser.read_u16()?;
            let _crc_val = parser.read_u32()?;
            let compressed_size = parser.read_u32()?;
            let uncompressed_size = parser.read_u32()?;
            let name_len = parser.read_u16()? as usize;
            let extra_len = parser.read_u16()? as usize;
            let comment_len = parser.read_u16()? as usize;
            let _disk_start = parser.read_u16()?;
            let _int_attrs = parser.read_u16()?;
            let _ext_attrs = parser.read_u32()?;
            let local_offset = parser.read_u32()?;
            if name_len > limits.max_path_len {
                return Err(ArchiveError::PathTooLong("?".into()));
            }
            if parser.remaining() < name_len + extra_len + comment_len {
                return Err(ArchiveError::Truncated);
            }
            let raw_name = parser.read(name_len)?;
            parser.skip(extra_len + comment_len)?;
            let name = std::str::from_utf8(raw_name)
                .map_err(|_| ArchiveError::Corrupt("non-utf8 name".into()))?;
            let name = normalize_path(name)?;
            if name.len() > limits.max_path_len {
                return Err(ArchiveError::PathTooLong(name));
            }
            if entries.contains_key(&name) {
                return Err(ArchiveError::DuplicateEntry(name));
            }
            if needed > 45 {
                // We don't support anything requiring newer features.
                return Err(ArchiveError::UnsupportedVersion(needed));
            }
            if method != 0 {
                return Err(ArchiveError::UnsupportedMethod(method));
            }
            if local_offset as usize > data.len() {
                return Err(ArchiveError::Truncated);
            }
            // The local header must be fully inside the archive. We do NOT
            // compare it against the central directory cursor position; that
            // comparison is meaningless and would reject valid archives.
            let local = read_local_header(&mut Cursor {
                data,
                pos: local_offset as usize,
            })?;
            if local.method != 0 {
                return Err(ArchiveError::UnsupportedMethod(local.method));
            }
            if local.compressed_size as u64 != compressed_size as u64
                || local.uncompressed_size as u64 != uncompressed_size as u64
            {
                return Err(ArchiveError::Corrupt("size mismatch local/central".into()));
            }
            if usize::from(local.name_len) != name_len {
                return Err(ArchiveError::Corrupt("name length mismatch".into()));
            }
            let data_start =
                local_offset as usize + 30 + local.name_len as usize + local.extra_len as usize;
            let data_end = data_start + local.compressed_size as usize;
            if data_end > data.len() {
                return Err(ArchiveError::Truncated);
            }
            let content = &data[data_start..data_end];
            if content.len() as u64 > limits.max_entry_size {
                return Err(ArchiveError::EntryTooLarge(name, content.len() as u64));
            }
            total_size += content.len() as u64;
            if total_size > limits.max_total_size {
                return Err(ArchiveError::TotalTooLarge(total_size));
            }
            if crc.checksum(content) != local.crc {
                return Err(ArchiveError::ChecksumMismatch(name));
            }
            entries.insert(name, content.to_vec());
        }
        Ok(Self { entries, limits })
    }

    /// Validate every entry against a manifest.
    pub fn validate_manifest(
        &self,
        manifest: &crate::manifest::Manifest,
    ) -> Result<(), ArchiveError> {
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for e in &manifest.entries {
            let data = self
                .get(&e.path)
                .ok_or_else(|| ArchiveError::Corrupt(format!("missing entry {}", e.path)))?;
            if data.len() as u64 != e.size {
                return Err(ArchiveError::Corrupt(format!(
                    "size mismatch for {}",
                    e.path
                )));
            }
            let actual = Checksum::from_bytes(sha256(data));
            if actual != e.sha256 {
                return Err(ArchiveError::ChecksumMismatch(e.path.clone()));
            }
            seen.insert(e.path.as_str());
        }
        // Ensure no extra content/ entries exist beyond the manifest.
        for p in self.paths() {
            if p.starts_with("content/") && !seen.contains(p) {
                return Err(ArchiveError::Corrupt(format!("unlisted content {p}")));
            }
        }
        Ok(())
    }
}

impl Default for PackageArchive {
    fn default() -> Self {
        Self::new()
    }
}

fn read_local_header(c: &mut Cursor<'_>) -> Result<LocalHeader, ArchiveError> {
    let sig = c.read_u32()?;
    if sig != 0x04034b50 {
        return Err(ArchiveError::BadSignature {
            expected: "local file header",
            offset: c.pos as u64 - 4,
        });
    }
    let _ver = c.read_u16()?;
    let _flags = c.read_u16()?;
    let method = c.read_u16()?;
    let _time = c.read_u16()?;
    let _date = c.read_u16()?;
    let crc = c.read_u32()?;
    let compressed_size = c.read_u32()?;
    let uncompressed_size = c.read_u32()?;
    let name_len = c.read_u16()?;
    let extra_len = c.read_u16()?;
    Ok(LocalHeader {
        method,
        crc,
        compressed_size,
        uncompressed_size,
        name_len,
        extra_len,
    })
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn read_u16(&mut self) -> Result<u16, ArchiveError> {
        self.read(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }

    fn read_u32(&mut self) -> Result<u32, ArchiveError> {
        self.read(4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn read(&mut self, n: usize) -> Result<&'a [u8], ArchiveError> {
        if self.remaining() < n {
            return Err(ArchiveError::Truncated);
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn skip(&mut self, n: usize) -> Result<(), ArchiveError> {
        if self.remaining() < n {
            return Err(ArchiveError::Truncated);
        }
        self.pos += n;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{Manifest, ManifestEntry, MimeType, PackageKind, SchemaVersion};

    #[test]
    fn crc32_known_vector() {
        let crc = Crc32::new();
        assert_eq!(crc.checksum(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc.checksum(b""), 0);
    }

    #[test]
    fn sha256_known_vector() {
        let h = sha256(b"abc");
        assert_eq!(
            hex(&h),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let h = sha256(b"");
        assert_eq!(
            hex(&h),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    #[test]
    fn normalize_paths() {
        assert_eq!(normalize_path("a/b/c").unwrap(), "a/b/c");
        assert_eq!(normalize_path("a/./b").unwrap(), "a/b");
        assert_eq!(normalize_path("a/../b").unwrap(), "b");
        assert!(normalize_path("../a").is_err());
        assert!(normalize_path("/a").is_err());
        assert!(normalize_path("a\\b").is_err());
        assert!(normalize_path("a\0b").is_err());
        assert!(normalize_path("").is_err());
    }

    #[test]
    fn round_trip() {
        let mut a = PackageArchive::new();
        a.add("content/document.json", br#"{"x":1}"#.to_vec())
            .unwrap();
        a.add("previews/thumb.png", vec![0u8; 64]).unwrap();
        let bytes = a.to_bytes().unwrap();
        let b = PackageArchive::from_bytes(&bytes).unwrap();
        assert_eq!(
            b.paths(),
            vec!["content/document.json", "previews/thumb.png"]
        );
        assert_eq!(b.get("content/document.json").unwrap(), br#"{"x":1}"#);
        assert_eq!(b.len(), 2);
    }

    #[test]
    fn detects_corruption() {
        let mut a = PackageArchive::new();
        a.add("content/x", b"hello".to_vec()).unwrap();
        let mut bytes = a.to_bytes().unwrap();
        // Flip a bit in the content.
        let content_pos = bytes.iter().position(|&b| b == b'h').unwrap();
        bytes[content_pos] ^= 0xFF;
        assert!(PackageArchive::from_bytes(&bytes).is_err());
    }

    #[test]
    fn duplicate_rejected() {
        let mut a = PackageArchive::new();
        a.add("a", vec![1]).unwrap();
        assert!(a.add("a", vec![2]).is_err());
        assert!(a.add("./a", vec![3]).is_err()); // normalizes to a
    }

    #[test]
    fn too_many_entries() {
        let mut a = PackageArchive::with_limits(ArchiveLimits {
            max_entries: 2,
            ..Default::default()
        });
        a.add("a", vec![1]).unwrap();
        a.add("b", vec![2]).unwrap();
        assert!(a.add("c", vec![3]).is_err());
    }

    #[test]
    fn entry_too_large() {
        let mut a = PackageArchive::with_limits(ArchiveLimits {
            max_entry_size: 4,
            ..Default::default()
        });
        assert!(a.add("big", vec![0; 8]).is_err());
    }

    #[test]
    fn validates_against_manifest() {
        let mut a = PackageArchive::new();
        let content = b"doc-body".to_vec();
        a.add("content/document.json", content.clone()).unwrap();
        let manifest = Manifest {
            schema: SchemaVersion::CURRENT,
            kind: PackageKind::Writer,
            id: "doc-1".into(),
            title: "T".into(),
            app_version: "0.1.0".into(),
            entries: vec![ManifestEntry {
                path: "content/document.json".into(),
                mime: MimeType::parse("application/json").unwrap(),
                size: content.len() as u64,
                sha256: Checksum::from_bytes(sha256(&content)),
            }],
        };
        a.validate_manifest(&manifest).unwrap();
    }

    #[test]
    fn manifest_mismatch_detected() {
        let mut a = PackageArchive::new();
        let content = b"doc-body".to_vec();
        a.add("content/document.json", content.clone()).unwrap();
        let manifest = Manifest {
            schema: SchemaVersion::CURRENT,
            kind: PackageKind::Writer,
            id: "doc-1".into(),
            title: "T".into(),
            app_version: "0.1.0".into(),
            entries: vec![ManifestEntry {
                path: "content/document.json".into(),
                mime: MimeType::parse("application/json").unwrap(),
                size: content.len() as u64,
                // Wrong checksum.
                sha256: Checksum::from_bytes([0u8; 32]),
            }],
        };
        assert!(a.validate_manifest(&manifest).is_err());
    }
}
