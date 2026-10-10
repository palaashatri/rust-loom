//! The font catalogue: what is installed, which face a request resolves to,
//! and lazily loaded faces for shaping.

use crate::bundled::bundled_bytes;
use crate::face::{FontError, LoadedFace, MAX_FONT_FILE_BYTES};
use crate::info::{normalize_family, FaceId, FaceInfo, FaceSource};
use crate::resolve::{
    best_match, generic_chain, instance_candidate, instance_variations, metric_compatible,
    Candidate, FaceSetup, FallbackPolicy, FontRef,
};
use crate::scan::{scan, ScanCache, ScanConfig, ScanReport};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// The family always appended to a fallback chain.
const LAST_RESORT_FAMILY: &str = "inter";

/// Default budget for font bytes held in memory (CJK fonts run to 20 MiB).
const DEFAULT_LOAD_BUDGET: usize = 192 << 20;

/// How a family name stands against the installed fonts; drives the
/// "font not installed" state of a font picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Availability {
    /// Installed on this system.
    Installed,
    /// Present only as one of the bundled faces.
    Bundled,
    /// A generic keyword such as `sans-serif`; always resolvable.
    Generic,
    /// Not installed, but `substitute` has identical metrics.
    MetricCompatible {
        /// The installed stand-in's family name.
        substitute: String,
    },
    /// Not installed; text will be drawn and measured with `substitute`.
    Missing {
        /// The installed stand-in's family name.
        substitute: String,
    },
}

struct LoadCache {
    faces: HashMap<FaceId, Arc<LoadedFace>>,
    order: VecDeque<FaceId>,
    sizes: HashMap<FaceId, usize>,
    bytes: usize,
    budget: usize,
}

impl LoadCache {
    fn new(budget: usize) -> Self {
        Self {
            faces: HashMap::new(),
            order: VecDeque::new(),
            sizes: HashMap::new(),
            bytes: 0,
            budget,
        }
    }

    /// Keeps `face` when it fits the budget. Bytes compiled into the binary
    /// cost no heap and count as zero. A face larger than the whole budget is
    /// not retained at all, so `bytes` never exceeds `budget`.
    fn insert(&mut self, id: FaceId, face: Arc<LoadedFace>, size: usize) {
        if size > self.budget {
            return;
        }
        if let Some(old) = self.faces.insert(id, face) {
            drop(old);
            self.bytes -= self.sizes.insert(id, size).unwrap_or(0);
            self.order.retain(|queued| *queued != id);
        } else {
            self.sizes.insert(id, size);
        }
        self.order.push_back(id);
        self.bytes += size;
        self.evict_to_budget();
    }

    /// Drops the oldest faces until the retained bytes fit the budget.
    fn evict_to_budget(&mut self) {
        while self.bytes > self.budget {
            let Some(old) = self.order.pop_front() else {
                break;
            };
            self.faces.remove(&old);
            self.bytes -= self.sizes.remove(&old).unwrap_or(0);
        }
    }
}

/// The installed fonts of one scan.
///
/// A catalogue is immutable after construction apart from its internal cache
/// of loaded faces, so it can be shared between threads behind an `Arc`.
/// Re-scan on a background thread and swap the `Arc` when the user installs a
/// font.
pub struct FontCatalog {
    faces: Vec<FaceInfo>,
    /// Lower-cased family name (typographic and legacy) to face indices.
    by_name: HashMap<String, Vec<u32>>,
    families: Vec<String>,
    policy: FallbackPolicy,
    report: ScanReport,
    loaded: Mutex<LoadCache>,
}

impl FontCatalog {
    /// A catalogue with no faces; useful before the first scan finishes.
    pub fn empty(policy: FallbackPolicy) -> Self {
        Self::build(Vec::new(), policy, ScanReport::default())
    }

    /// A catalogue holding only the bundled Inter faces. Instant, and the
    /// right placeholder while a system scan runs on another thread.
    pub fn bundled_only() -> Self {
        let mut config = ScanConfig::with_dirs(Vec::new());
        config.include_bundled = true;
        Self::scan(&config)
    }

    /// Scans the running system's font directories.
    pub fn system() -> Self {
        Self::scan(&ScanConfig::system())
    }

    /// Scans `config` without a persistent cache.
    pub fn scan(config: &ScanConfig) -> Self {
        Self::scan_with_cache(config, &mut ScanCache::new())
    }

    /// Scans `config`, reusing and updating `cache`. Files whose size and
    /// modification time are unchanged are not opened.
    pub fn scan_with_cache(config: &ScanConfig, cache: &mut ScanCache) -> Self {
        let scanned = scan(config, cache);
        Self::build(scanned.faces, config.policy.clone(), scanned.report)
    }

    fn build(found: Vec<FaceInfo>, policy: FallbackPolicy, mut report: ScanReport) -> Self {
        // First sighting wins, so directory order is priority order and the
        // bundled faces (listed first) take precedence over installed copies.
        let mut seen = HashSet::new();
        let mut faces: Vec<FaceInfo> = found
            .into_iter()
            .filter(|face| seen.insert(face.dedupe_key()))
            .collect();
        faces.sort_by(|a, b| {
            let key = |f: &FaceInfo| {
                (
                    f.family.to_lowercase(),
                    f.weight,
                    f.italic,
                    f.width,
                    f.postscript_name.to_lowercase(),
                )
            };
            key(a)
                .cmp(&key(b))
                .then_with(|| source_key(a).cmp(&source_key(b)))
        });
        report.faces = faces.len();

        let mut by_name: HashMap<String, Vec<u32>> = HashMap::new();
        let mut families: Vec<String> = Vec::new();
        let mut family_seen = HashSet::new();
        for (index, face) in faces.iter().enumerate() {
            let index = index as u32;
            let names = [&face.family, &face.legacy_family]
                .into_iter()
                .chain(&face.aliases);
            for name in names {
                let list = by_name.entry(normalize_family(name)).or_default();
                if list.last() != Some(&index) {
                    list.push(index);
                }
            }
            if family_seen.insert(normalize_family(&face.family)) {
                families.push(face.family.clone());
            }
        }
        families.sort_by_key(|f| f.to_lowercase());

        Self {
            faces,
            by_name,
            families,
            policy,
            report,
            loaded: Mutex::new(LoadCache::new(DEFAULT_LOAD_BUDGET)),
        }
    }

    /// What the scan that built this catalogue did.
    pub fn report(&self) -> &ScanReport {
        &self.report
    }

    /// The fallback policy in force.
    pub fn policy(&self) -> &FallbackPolicy {
        &self.policy
    }

    /// Changes the memory budget for loaded font bytes.
    ///
    /// The budget is a hard ceiling on retained heap bytes: lowering it evicts
    /// at once, and a face larger than the whole budget is still returned by
    /// [`FontCatalog::load`] but is not kept (so it is re-read on every load).
    /// Bundled faces live in the binary and do not count.
    pub fn set_load_budget(&self, bytes: usize) {
        if let Ok(mut cache) = self.loaded.lock() {
            cache.budget = bytes;
            cache.evict_to_budget();
        }
    }

    /// True when the catalogue has no faces.
    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    /// Number of faces.
    pub fn face_count(&self) -> usize {
        self.faces.len()
    }

    /// Family names (typographic), sorted case-insensitively, without
    /// duplicates. This is the font picker's list.
    pub fn families(&self) -> &[String] {
        &self.families
    }

    /// True when `family` names any installed or bundled face.
    pub fn has_family(&self, family: &str) -> bool {
        self.by_name.contains_key(&normalize_family(family))
    }

    /// The faces of `family` (matched case-insensitively against both the
    /// typographic and the legacy style-linked name), lightest first, upright
    /// before italic.
    pub fn faces(&self, family: &str) -> Vec<(FaceId, &FaceInfo)> {
        let mut found: Vec<(FaceId, &FaceInfo)> = self
            .by_name
            .get(&normalize_family(family))
            .map(|indices| {
                indices
                    .iter()
                    .map(|&i| (FaceId(i), &self.faces[i as usize]))
                    .collect()
            })
            .unwrap_or_default();
        found.sort_by_key(|(id, face)| (face.weight, face.italic, id.0));
        found
    }

    /// Every face in catalogue order (family, then weight, then style).
    pub fn faces_in_order(&self) -> impl Iterator<Item = (FaceId, &FaceInfo)> {
        self.faces
            .iter()
            .enumerate()
            .map(|(i, face)| (FaceId(i as u32), face))
    }

    /// The description of `id`, or `None` for a handle from another catalogue.
    pub fn face(&self, id: FaceId) -> Option<&FaceInfo> {
        self.faces.get(id.index())
    }

    /// The best face of the family called `normalized` for the request.
    ///
    /// A variable font contributes its default instance and each of its named
    /// instances as separate candidates, so a request for bold is met by the
    /// font's Bold instance (and reported with that instance's coordinates)
    /// instead of being marked for synthetic emboldening.
    fn match_family(
        &self,
        normalized: &str,
        weight: u16,
        italic: bool,
    ) -> Option<(FaceId, FaceSetup)> {
        let indices = self.by_name.get(normalized)?;
        let mut candidates: Vec<Candidate> = Vec::new();
        // Which face, and which of its named instances, each candidate is.
        let mut origin: Vec<(u32, Option<usize>)> = Vec::new();
        for &i in indices {
            let face = &self.faces[i as usize];
            let base = Candidate {
                weight: face.weight,
                width: face.width,
                italic: face.italic,
            };
            candidates.push(base);
            origin.push((i, None));
            for (k, instance) in face.instances.iter().enumerate() {
                candidates.push(instance_candidate(base, &face.axes, &instance.coords));
                origin.push((i, Some(k)));
            }
        }
        let found = best_match(&candidates, weight, italic)?;
        let (face_index, instance) = origin[found.index];
        let variations = instance
            .map(|k| {
                let face = &self.faces[face_index as usize];
                instance_variations(&face.axes, &face.instances[k].coords)
            })
            .unwrap_or_default();
        Some((
            FaceId(face_index),
            FaceSetup {
                variations,
                synthetic_bold: found.synthetic_bold,
                synthetic_italic: found.synthetic_italic,
            },
        ))
    }

    /// Resolves a document's font request.
    ///
    /// The result never fails to name a face unless the catalogue is empty:
    /// a missing family falls back through metric-compatible stand-ins, the
    /// policy list and finally the bundled Inter, and says so through
    /// [`FontRef::substituted`]. The document should keep the family it asked
    /// for; only measurement and drawing use the stand-in.
    pub fn resolve(&self, family: &str, weight: u16, italic: bool) -> Result<FontRef, FontError> {
        if self.faces.is_empty() {
            return Err(FontError::EmptyCatalog);
        }
        let normalized = normalize_family(family);
        let mut metric_compatible_hit = false;
        let mut substituted = false;

        let mut primary = self.match_family(&normalized, weight, italic);
        if primary.is_none() && generic_chain(&normalized).is_some() {
            // A keyword asks for "some sans-serif", so whatever the policy and
            // bundled fallbacks give is the answer, not a substitution.
            primary = generic_chain(&normalized)
                .into_iter()
                .flatten()
                .find_map(|name| self.match_family(name, weight, italic))
                .or_else(|| {
                    self.fallback_names()
                        .find_map(|name| self.match_family(&name, weight, italic))
                });
        }
        if primary.is_none() {
            primary = metric_compatible(&normalized)
                .iter()
                .find_map(|name| self.match_family(name, weight, italic));
            if primary.is_some() {
                substituted = true;
                metric_compatible_hit = true;
            }
        }
        if primary.is_none() {
            substituted = true;
            primary = self
                .fallback_names()
                .find_map(|name| self.match_family(&name, weight, italic));
        }
        // Whatever is left: the first face of the catalogue.
        let (first, first_setup) = primary.unwrap_or((
            FaceId(0),
            FaceSetup {
                variations: Vec::new(),
                synthetic_bold: weight >= 600,
                synthetic_italic: italic,
            },
        ));

        let mut chain = vec![first];
        let mut chain_setup = vec![first_setup.clone()];
        for name in self.fallback_names() {
            if let Some((id, setup)) = self.match_family(&name, weight, italic) {
                if !chain.contains(&id) {
                    chain.push(id);
                    chain_setup.push(setup);
                }
            }
        }
        Ok(FontRef {
            requested_family: family.to_owned(),
            requested_weight: weight,
            requested_italic: italic,
            chain,
            substituted,
            metric_compatible: metric_compatible_hit,
            synthetic_bold: first_setup.synthetic_bold,
            synthetic_italic: first_setup.synthetic_italic,
            chain_setup,
        })
    }

    /// Policy families, then the last-resort family, normalised.
    fn fallback_names(&self) -> impl Iterator<Item = String> + '_ {
        self.policy
            .families()
            .iter()
            .map(|f| normalize_family(f))
            .chain(std::iter::once(LAST_RESORT_FAMILY.to_owned()))
    }

    /// How `family` stands against what is installed.
    pub fn availability(&self, family: &str) -> Availability {
        let normalized = normalize_family(family);
        let installed: Vec<&FaceInfo> = self
            .by_name
            .get(&normalized)
            .map(|indices| indices.iter().map(|&i| &self.faces[i as usize]).collect())
            .unwrap_or_default();
        if !installed.is_empty() {
            let any_file = installed
                .iter()
                .any(|f| matches!(f.source, FaceSource::File(_)));
            return if any_file {
                Availability::Installed
            } else {
                Availability::Bundled
            };
        }
        if generic_chain(&normalized).is_some() {
            return Availability::Generic;
        }
        let resolved = self.resolve(family, 400, false).ok();
        let substitute = resolved
            .as_ref()
            .and_then(|r| self.face(r.primary()))
            .map(|f| f.family.clone())
            .unwrap_or_default();
        if resolved.is_some_and(|r| r.metric_compatible) {
            Availability::MetricCompatible { substitute }
        } else {
            Availability::Missing { substitute }
        }
    }

    /// Loads `id` (reading its file the first time) and caches it.
    pub fn load(&self, id: FaceId) -> Result<Arc<LoadedFace>, FontError> {
        if let Ok(cache) = self.loaded.lock() {
            if let Some(face) = cache.faces.get(&id) {
                return Ok(Arc::clone(face));
            }
        }
        let info = self
            .face(id)
            .ok_or_else(|| FontError::Parse("unknown face handle".into()))?;
        let (face, retained) = match &info.source {
            FaceSource::Bundled(name) => {
                let bytes = bundled_bytes(name)
                    .ok_or_else(|| FontError::Io(format!("bundled face {name} is missing")))?;
                (LoadedFace::from_static(bytes, info.index)?, 0)
            }
            FaceSource::File(path) => {
                let bytes = read_capped(path)?;
                let len = bytes.len();
                (LoadedFace::from_vec(bytes, info.index)?, len)
            }
        };
        let face = Arc::new(face);
        if let Ok(mut cache) = self.loaded.lock() {
            cache.insert(id, Arc::clone(&face), retained);
        }
        Ok(face)
    }

    /// Bytes of font data currently held in memory by the load cache.
    pub fn loaded_bytes(&self) -> usize {
        self.loaded.lock().map_or(0, |c| c.bytes)
    }

    /// Number of faces currently held in memory.
    pub fn loaded_count(&self) -> usize {
        self.loaded.lock().map_or(0, |c| c.faces.len())
    }

    /// True when face `id` has a glyph for `ch` (see [`LoadedFace::covers`]).
    pub fn covers(&self, id: FaceId, ch: char) -> bool {
        self.load(id).is_ok_and(|face| face.covers(ch))
    }

    /// The first face of the chain that covers `ch`, or the primary face.
    pub fn face_for_char(&self, font: &FontRef, ch: char) -> FaceId {
        font.chain
            .iter()
            .copied()
            .find(|id| self.covers(*id, ch))
            .unwrap_or(font.chain[0])
    }
}

/// Reads a font file, refusing anything over [`MAX_FONT_FILE_BYTES`] without
/// allocating for it. The length is checked from metadata first and again on
/// the bytes actually read, so a file that grows between the two is caught.
fn read_capped(path: &Path) -> Result<Vec<u8>, FontError> {
    let io = |error: std::io::Error| FontError::Io(error.to_string());
    let file = File::open(path).map_err(io)?;
    let len = file.metadata().map_err(io)?.len();
    read_limited(file, len, MAX_FONT_FILE_BYTES)
}

/// Reads at most `limit` bytes from `reader`, which is said to hold
/// `declared` bytes. A declared length over the limit is refused without
/// reading; a reader that turns out to hold more than the limit is refused
/// after reading at most `limit + 1` bytes.
fn read_limited(reader: impl Read, declared: u64, limit: u64) -> Result<Vec<u8>, FontError> {
    if declared > limit {
        return Err(FontError::TooLarge);
    }
    let mut bytes = Vec::with_capacity(usize::try_from(declared).unwrap_or(0));
    reader
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| FontError::Io(error.to_string()))?;
    if bytes.len() as u64 > limit {
        return Err(FontError::TooLarge);
    }
    Ok(bytes)
}

fn source_key(face: &FaceInfo) -> String {
    match &face.source {
        FaceSource::Bundled(name) => format!("0{name}"),
        FaceSource::File(path) => format!("1{}#{}", path.to_string_lossy(), face.index),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn a_declared_length_over_the_limit_is_refused_unread() {
        struct Unreadable;
        impl Read for Unreadable {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                panic!("must not be read");
            }
        }
        assert_eq!(read_limited(Unreadable, 101, 100), Err(FontError::TooLarge));
    }

    #[test]
    fn a_file_that_grows_past_its_declared_length_is_still_capped() {
        // Declared 10 bytes but actually 500: the read stops at limit + 1.
        let data = vec![7u8; 500];
        assert_eq!(
            read_limited(Cursor::new(&data), 10, 100),
            Err(FontError::TooLarge)
        );
        // Exactly the limit is allowed, one more is not.
        assert_eq!(
            read_limited(Cursor::new(&data[..100]), 100, 100).map(|b| b.len()),
            Ok(100)
        );
        assert_eq!(
            read_limited(Cursor::new(&data[..101]), 100, 100),
            Err(FontError::TooLarge)
        );
    }
}
