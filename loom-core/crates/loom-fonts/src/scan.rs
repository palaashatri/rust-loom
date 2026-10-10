//! Finding font files and describing them, with an optional persistent cache.

use crate::bundled::bundled_faces;
use crate::info::{EmbeddingPermission, FaceInfo, FaceSource, NamedInstance, VariationAxis};
use crate::resolve::FallbackPolicy;
use crate::sfnt::{read_faces, FileReader, SliceReader};
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};

/// Operating-system family used to pick the default font directories.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    /// Windows.
    Windows,
    /// macOS.
    MacOs,
    /// Linux and other Unix systems using fontconfig-style directories.
    Unix,
}

impl Platform {
    /// The platform this binary runs on.
    pub fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Unix
        }
    }
}

/// The directories that hold installed fonts, in priority order (an earlier
/// directory wins when the same face appears twice). `env` looks up an
/// environment variable; tests pass a fake one.
pub fn system_font_dirs(
    platform: Platform,
    env: &dyn Fn(&str) -> Option<OsString>,
) -> Vec<PathBuf> {
    let var = |name: &str| env(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    let mut dirs: Vec<PathBuf> = Vec::new();
    match platform {
        Platform::Windows => {
            let windir = var("WINDIR")
                .or_else(|| var("SystemRoot"))
                .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
            dirs.push(windir.join("Fonts"));
            if let Some(local) = var("LOCALAPPDATA") {
                dirs.push(local.join("Microsoft").join("Windows").join("Fonts"));
            }
        }
        Platform::MacOs => {
            dirs.push("/System/Library/Fonts".into());
            dirs.push("/System/Library/Fonts/Supplemental".into());
            dirs.push("/Library/Fonts".into());
            if let Some(home) = var("HOME") {
                dirs.push(home.join("Library").join("Fonts"));
            }
        }
        Platform::Unix => {
            dirs.push("/usr/share/fonts".into());
            dirs.push("/usr/local/share/fonts".into());
            let home = var("HOME");
            let data_home = var("XDG_DATA_HOME")
                .or_else(|| home.as_ref().map(|h| h.join(".local").join("share")));
            if let Some(data_home) = data_home {
                dirs.push(data_home.join("fonts"));
            }
            if let Some(home) = &home {
                dirs.push(home.join(".fonts"));
            }
            let data_dirs = env("XDG_DATA_DIRS")
                .and_then(|v| v.into_string().ok())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned());
            for dir in data_dirs.split(':').filter(|d| !d.is_empty()) {
                dirs.push(Path::new(dir).join("fonts"));
            }
        }
    }
    let mut seen = HashSet::new();
    dirs.retain(|d| seen.insert(d.clone()));
    dirs
}

/// Bounds that keep a scan from running away on a huge or slow volume.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanLimits {
    /// Most font files examined in one scan.
    pub max_files: usize,
    /// Deepest directory nesting entered below a root.
    pub max_depth: usize,
    /// Wall-clock budget; the scan stops and reports `truncated` after it.
    pub max_duration: Option<Duration>,
    /// Most directory entries (files, directories and everything else)
    /// examined in one scan. Entries are streamed, so a directory holding
    /// millions of unrelated files stops the scan here instead of exhausting
    /// memory.
    pub max_entries: usize,
}

impl Default for ScanLimits {
    fn default() -> Self {
        Self {
            max_files: 20_000,
            max_depth: 8,
            max_duration: Some(Duration::from_secs(20)),
            max_entries: 200_000,
        }
    }
}

/// What to scan.
#[derive(Clone, Debug)]
pub struct ScanConfig {
    /// Directories searched recursively, highest priority first.
    pub dirs: Vec<PathBuf>,
    /// Add the bundled Inter faces ahead of everything else.
    pub include_bundled: bool,
    /// Scan bounds.
    pub limits: ScanLimits,
    /// Where to look when a family or character is missing.
    pub policy: FallbackPolicy,
}

impl ScanConfig {
    /// The directories of the running system, read from the environment.
    pub fn system() -> Self {
        let env = |name: &str| std::env::var_os(name);
        Self::with_dirs(system_font_dirs(Platform::current(), &env))
    }

    /// An explicit directory list (the deterministic form tests use).
    pub fn with_dirs(dirs: Vec<PathBuf>) -> Self {
        Self {
            dirs,
            include_bundled: true,
            limits: ScanLimits::default(),
            policy: FallbackPolicy::platform_default(),
        }
    }
}

/// A file or directory that was looked at and skipped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanIssue {
    /// The file or directory.
    pub path: PathBuf,
    /// Why it was skipped.
    pub reason: String,
}

/// What a scan did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScanReport {
    /// Directory entries examined while walking the directories.
    pub entries_examined: usize,
    /// Candidate font files found (by extension).
    pub files_seen: usize,
    /// Files whose tables were read.
    pub files_parsed: usize,
    /// Files answered from the cache without reading them.
    pub cache_hits: usize,
    /// Faces in the finished catalogue after removing duplicates.
    pub faces: usize,
    /// Files skipped, with reasons.
    pub issues: Vec<ScanIssue>,
    /// True when a limit stopped the scan early; the catalogue is partial.
    pub truncated: bool,
    /// Time the scan took.
    pub elapsed: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CacheEntry {
    len: u64,
    modified_ns: u64,
    faces: Vec<FaceInfo>,
}

/// Remembers per-file results between scans, keyed by path, size and
/// modification time, so a restart parses only what changed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScanCache {
    entries: HashMap<PathBuf, CacheEntry>,
}

const CACHE_HEADER: &str = "loom-fonts-cache\t2";

fn tag_hex(tag: [u8; 4]) -> String {
    tag.iter().map(|b| format!("{b:02x}")).collect()
}

fn parse_tag_hex(text: &str) -> Option<[u8; 4]> {
    if text.len() != 8 || !text.is_ascii() {
        return None;
    }
    let mut tag = [0u8; 4];
    for (slot, pair) in tag.iter_mut().zip(text.as_bytes().chunks(2)) {
        *slot = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(tag)
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

/// The most recently added face of the file being parsed.
fn last_face<'a>(
    entries: &'a mut HashMap<PathBuf, CacheEntry>,
    current: &Option<PathBuf>,
) -> Option<&'a mut FaceInfo> {
    entries.get_mut(current.as_ref()?)?.faces.last_mut()
}

impl ScanCache {
    /// An empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of files remembered.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when nothing is remembered.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// A tab-separated text form safe to store in a config directory.
    pub fn to_text(&self) -> String {
        let mut paths: Vec<&PathBuf> = self.entries.keys().collect();
        paths.sort();
        let mut out = String::from(CACHE_HEADER);
        out.push('\n');
        for path in paths {
            let entry = &self.entries[path];
            out.push_str(&format!(
                "file\t{}\t{}\t{}\n",
                escape(&path.to_string_lossy()),
                entry.len,
                entry.modified_ns
            ));
            for face in &entry.faces {
                let flags = u8::from(face.italic)
                    | u8::from(face.monospace) << 1
                    | u8::from(face.variable) << 2
                    | u8::from(face.subsetting_allowed) << 3;
                out.push_str(&format!(
                    "face\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                    face.index,
                    escape(&face.family),
                    escape(&face.legacy_family),
                    escape(&face.style),
                    escape(&face.full_name),
                    escape(&face.postscript_name),
                    face.weight,
                    face.width,
                    flags,
                    face.embedding.code(),
                ));
                for alias in &face.aliases {
                    out.push_str(&format!("alias\t{}\n", escape(alias)));
                }
                for axis in &face.axes {
                    out.push_str(&format!(
                        "axis\t{}\t{}\t{}\t{}\n",
                        tag_hex(axis.tag),
                        axis.min,
                        axis.default,
                        axis.max
                    ));
                }
                for instance in &face.instances {
                    out.push_str(&format!("instance\t{}", escape(&instance.style)));
                    for coord in &instance.coords {
                        out.push_str(&format!("\t{coord}"));
                    }
                    out.push('\n');
                }
            }
        }
        out
    }

    /// Parses [`ScanCache::to_text`] output. Anything unrecognised yields an
    /// empty cache, which only costs a full re-scan.
    pub fn from_text(text: &str) -> Self {
        let mut lines = text.lines();
        if lines.next() != Some(CACHE_HEADER) {
            return Self::new();
        }
        let mut entries: HashMap<PathBuf, CacheEntry> = HashMap::new();
        let mut current: Option<PathBuf> = None;
        for line in lines {
            let fields: Vec<&str> = line.split('\t').collect();
            match fields.as_slice() {
                ["file", path, len, modified] => {
                    let (Ok(len), Ok(modified_ns)) = (len.parse(), modified.parse()) else {
                        return Self::new();
                    };
                    let path = PathBuf::from(unescape(path));
                    entries.insert(
                        path.clone(),
                        CacheEntry {
                            len,
                            modified_ns,
                            faces: Vec::new(),
                        },
                    );
                    current = Some(path);
                }
                ["face", index, family, legacy, style, full, ps, weight, width, flags, embed] => {
                    let Some(path) = current.clone() else {
                        return Self::new();
                    };
                    let parsed = (
                        index.parse::<u32>(),
                        weight.parse::<u16>(),
                        width.parse::<u16>(),
                        flags.parse::<u8>(),
                        embed.parse::<u8>(),
                    );
                    let (Ok(index), Ok(weight), Ok(width), Ok(flags), Ok(embed)) = parsed else {
                        return Self::new();
                    };
                    if let Some(entry) = entries.get_mut(&path) {
                        entry.faces.push(FaceInfo {
                            family: unescape(family),
                            legacy_family: unescape(legacy),
                            aliases: Vec::new(),
                            axes: Vec::new(),
                            instances: Vec::new(),
                            style: unescape(style),
                            full_name: unescape(full),
                            postscript_name: unescape(ps),
                            weight,
                            width,
                            italic: flags & 1 != 0,
                            monospace: flags & 2 != 0,
                            variable: flags & 4 != 0,
                            embedding: EmbeddingPermission::from_code(embed),
                            subsetting_allowed: flags & 8 != 0,
                            source: FaceSource::File(path),
                            index,
                        });
                    }
                }
                ["alias", name] => {
                    let Some(face) = last_face(&mut entries, &current) else {
                        return Self::new();
                    };
                    face.aliases.push(unescape(name));
                }
                ["axis", tag, min, default, max] => {
                    let parsed = (
                        parse_tag_hex(tag),
                        min.parse::<f32>(),
                        default.parse::<f32>(),
                        max.parse::<f32>(),
                    );
                    let (Some(tag), Ok(min), Ok(default), Ok(max)) = parsed else {
                        return Self::new();
                    };
                    let Some(face) = last_face(&mut entries, &current) else {
                        return Self::new();
                    };
                    face.axes.push(VariationAxis {
                        tag,
                        min,
                        default,
                        max,
                    });
                }
                ["instance", style, coords @ ..] => {
                    let coords: Result<Vec<f32>, _> = coords.iter().map(|c| c.parse()).collect();
                    let Ok(coords) = coords else {
                        return Self::new();
                    };
                    let Some(face) = last_face(&mut entries, &current) else {
                        return Self::new();
                    };
                    face.instances.push(NamedInstance {
                        style: unescape(style),
                        coords,
                    });
                }
                _ => return Self::new(),
            }
        }
        Self { entries }
    }
}

fn is_font_file(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        ["ttf", "otf", "ttc", "otc"]
            .iter()
            .any(|known| e.eq_ignore_ascii_case(known))
    })
}

struct Walk<'a> {
    limits: &'a ScanLimits,
    start: Instant,
    files: Vec<PathBuf>,
    issues: Vec<ScanIssue>,
    /// Canonical paths of the directories already entered, so a symlink to an
    /// ancestor (or to a directory reached another way) is not entered twice.
    visited: HashSet<PathBuf>,
    entries: usize,
    truncated: bool,
}

/// What one directory entry is, after following a symlink.
enum EntryKind {
    File,
    Dir,
    Other,
}

fn classify(entry: &fs::DirEntry, path: &Path) -> EntryKind {
    let Ok(kind) = entry.file_type() else {
        return EntryKind::Other;
    };
    let resolved = if kind.is_symlink() {
        // Symlinked files and directories are followed. A dangling link
        // resolves to nothing and is skipped.
        fs::metadata(path).ok().map(|m| (m.is_file(), m.is_dir()))
    } else {
        Some((kind.is_file(), kind.is_dir()))
    };
    match resolved {
        Some((true, _)) => EntryKind::File,
        Some((_, true)) => EntryKind::Dir,
        _ => EntryKind::Other,
    }
}

impl Walk<'_> {
    fn out_of_time(&self) -> bool {
        self.limits
            .max_duration
            .is_some_and(|limit| self.start.elapsed() >= limit)
    }

    fn issue(&mut self, path: &Path, reason: String) {
        self.issues.push(ScanIssue {
            path: path.to_path_buf(),
            reason,
        });
    }

    /// Visits `dir`, files before subdirectories, both in name order, so a
    /// complete scan is deterministic whatever order the filesystem returns.
    ///
    /// Entries are streamed: the limits are checked before each one is
    /// examined, and only font files and subdirectories are kept, so a
    /// directory of a million unrelated files costs a counter, not a listing.
    fn visit(&mut self, dir: &Path, depth: usize) {
        if self.truncated {
            return;
        }
        if self.out_of_time() {
            self.truncated = true;
            return;
        }
        let canonical = fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
        if !self.visited.insert(canonical) {
            return;
        }
        let read = match fs::read_dir(dir) {
            Ok(read) => read,
            // A directory that is not there (a default location this
            // machine never created) is normal.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => {
                self.issue(dir, format!("directory could not be read: {error}"));
                return;
            }
        };
        let mut files: Vec<(std::ffi::OsString, PathBuf)> = Vec::new();
        // Real directories sort before symlinked ones, so a tree is entered
        // by its real path and the alias that points at it is skipped.
        let mut subdirs: Vec<(bool, std::ffi::OsString, PathBuf)> = Vec::new();
        for entry in read {
            if self.entries >= self.limits.max_entries || self.out_of_time() {
                self.truncated = true;
                break;
            }
            self.entries += 1;
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    self.issue(dir, format!("directory entry could not be read: {error}"));
                    continue;
                }
            };
            let path = entry.path();
            match classify(&entry, &path) {
                EntryKind::File if is_font_file(&path) => {
                    if self.files.len() + files.len() >= self.limits.max_files {
                        self.truncated = true;
                        break;
                    }
                    files.push((entry.file_name(), path));
                }
                EntryKind::Dir => {
                    let linked = entry.file_type().is_ok_and(|kind| kind.is_symlink());
                    subdirs.push((linked, entry.file_name(), path));
                }
                _ => {}
            }
        }
        files.sort();
        subdirs.sort();
        self.files.extend(files.into_iter().map(|(_, path)| path));
        if self.truncated || subdirs.is_empty() {
            return;
        }
        if depth >= self.limits.max_depth {
            // Directories exist below the limit: the catalogue is partial.
            self.truncated = true;
            return;
        }
        for (_, _, sub) in subdirs {
            self.visit(&sub, depth + 1);
            if self.truncated {
                return;
            }
        }
    }
}

fn modified_ns(metadata: &fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
}

/// Everything a scan discovered, before indexing.
pub(crate) struct Scanned {
    pub faces: Vec<FaceInfo>,
    pub report: ScanReport,
}

/// Runs a scan, consulting and updating `cache`.
pub(crate) fn scan(config: &ScanConfig, cache: &mut ScanCache) -> Scanned {
    let start = Instant::now();
    let mut report = ScanReport::default();
    let mut faces: Vec<FaceInfo> = Vec::new();

    if config.include_bundled {
        for (name, bytes) in bundled_faces() {
            match read_faces(&SliceReader(bytes), &FaceSource::Bundled(name)) {
                Ok(found) => faces.extend(found),
                Err(error) => report.issues.push(ScanIssue {
                    path: PathBuf::from(*name),
                    reason: error.to_string(),
                }),
            }
        }
    }

    let mut walk = Walk {
        limits: &config.limits,
        start,
        files: Vec::new(),
        issues: Vec::new(),
        visited: HashSet::new(),
        entries: 0,
        truncated: false,
    };
    for dir in &config.dirs {
        walk.visit(dir, 0);
    }
    report.truncated = walk.truncated;
    report.entries_examined = walk.entries;
    report.issues.append(&mut walk.issues);

    let mut seen_paths: HashSet<PathBuf> = HashSet::new();
    for path in walk.files {
        if !seen_paths.insert(path.clone()) {
            continue;
        }
        if config
            .limits
            .max_duration
            .is_some_and(|limit| start.elapsed() >= limit)
        {
            report.truncated = true;
            break;
        }
        report.files_seen += 1;
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        let (len, modified) = (metadata.len(), modified_ns(&metadata));
        if let Some(entry) = cache.entries.get(&path) {
            if entry.len == len && entry.modified_ns == modified {
                report.cache_hits += 1;
                faces.extend(entry.faces.iter().cloned());
                continue;
            }
        }
        report.files_parsed += 1;
        let source = FaceSource::File(path.clone());
        let parsed = FileReader::open(&path).and_then(|reader| read_faces(&reader, &source));
        let found = match parsed {
            Ok(found) => found,
            Err(error) => {
                report.issues.push(ScanIssue {
                    path: path.clone(),
                    reason: error.to_string(),
                });
                Vec::new()
            }
        };
        cache.entries.insert(
            path,
            CacheEntry {
                len,
                modified_ns: modified,
                faces: found.clone(),
            },
        );
        faces.extend(found);
    }
    if !report.truncated {
        cache.entries.retain(|path, _| seen_paths.contains(path));
    }
    report.elapsed = start.elapsed();
    Scanned { faces, report }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| OsString::from(*v))
        }
    }

    #[test]
    fn windows_dirs_use_windir_and_the_per_user_folder() {
        let env = fake_env(&[
            ("WINDIR", r"D:\Win"),
            ("LOCALAPPDATA", r"C:\Users\a\AppData\Local"),
        ]);
        let dirs = system_font_dirs(Platform::Windows, &env);
        assert_eq!(dirs[0], PathBuf::from(r"D:\Win").join("Fonts"));
        assert_eq!(
            dirs[1],
            PathBuf::from(r"C:\Users\a\AppData\Local")
                .join("Microsoft")
                .join("Windows")
                .join("Fonts")
        );
    }

    #[test]
    fn windows_falls_back_to_systemroot_then_c_windows() {
        let env = fake_env(&[("SystemRoot", r"E:\W")]);
        assert_eq!(
            system_font_dirs(Platform::Windows, &env)[0],
            PathBuf::from(r"E:\W").join("Fonts")
        );
        let none = fake_env(&[]);
        assert_eq!(
            system_font_dirs(Platform::Windows, &none),
            vec![PathBuf::from(r"C:\Windows").join("Fonts")]
        );
    }

    #[test]
    fn unix_dirs_follow_xdg_and_skip_duplicates() {
        let env = fake_env(&[
            ("HOME", "/home/u"),
            ("XDG_DATA_HOME", "/data"),
            ("XDG_DATA_DIRS", "/usr/share:/opt/share"),
        ]);
        let dirs = system_font_dirs(Platform::Unix, &env);
        assert_eq!(dirs[0], PathBuf::from("/usr/share/fonts"));
        assert!(dirs.contains(&PathBuf::from("/data").join("fonts")));
        assert!(dirs.contains(&PathBuf::from("/home/u").join(".fonts")));
        assert!(dirs.contains(&PathBuf::from("/opt/share").join("fonts")));
        let unique: HashSet<_> = dirs.iter().collect();
        assert_eq!(unique.len(), dirs.len());
        let home_only = fake_env(&[("HOME", "/home/u")]);
        let expected = PathBuf::from("/home/u")
            .join(".local")
            .join("share")
            .join("fonts");
        assert!(system_font_dirs(Platform::Unix, &home_only).contains(&expected));
    }

    #[test]
    fn macos_dirs_include_the_user_library() {
        let env = fake_env(&[("HOME", "/Users/u")]);
        let dirs = system_font_dirs(Platform::MacOs, &env);
        assert!(dirs.contains(&PathBuf::from("/Library/Fonts")));
        assert!(dirs.contains(&PathBuf::from("/Users/u").join("Library").join("Fonts")));
    }

    #[test]
    fn escaping_round_trips_awkward_names() {
        let name = "Odd\tName\\with\nbreaks";
        assert_eq!(unescape(&escape(name)), name);
    }

    #[test]
    fn extension_check_is_case_insensitive_and_strict() {
        assert!(is_font_file(Path::new("A.TTF")));
        assert!(is_font_file(Path::new("b.otc")));
        assert!(!is_font_file(Path::new("c.woff2")));
        assert!(!is_font_file(Path::new("fonts.dir")));
    }

    #[test]
    fn unrecognised_cache_text_is_an_empty_cache() {
        assert!(ScanCache::from_text("not a cache").is_empty());
        assert!(ScanCache::from_text("").is_empty());
    }
}
