//! Sparse cell storage split into row bands that are shared copy-on-write.
//!
//! [`BandedMap`] keeps a sheet's cells in bands of [`BAND_ROWS`] rows. Cloning
//! the map copies one reference per band, so an undo snapshot, a worker copy,
//! or the saved baseline costs the number of bands rather than the number of
//! cells. A write copies only the band it touches, and only while another copy
//! still shares that band. Each band also keeps the summaries every edit needs
//! (extent, formula count, persisted-content digest, byte estimate), so those
//! questions never scan the cells.

use std::collections::btree_map;
use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, OnceLock};

use crate::CellRef;

/// Rows per band. A write to a shared band copies at most this many rows.
pub const BAND_ROWS: u32 = 16;

/// Estimated bookkeeping bytes per stored entry (its share of a B-tree node).
const ENTRY_OVERHEAD_BYTES: usize = 32;
/// Estimated fixed bytes per band: the band, its reference count, and its tree.
const BAND_OVERHEAD_BYTES: usize = 128;

#[cfg(test)]
thread_local! {
    static BAND_COPIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static BAND_SCANS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Bands copied on this thread because a write found them shared (tests only).
#[cfg(test)]
pub(crate) fn band_copies_on_this_thread() -> usize {
    BAND_COPIES.with(std::cell::Cell::get)
}

/// Full-band scans run on this thread to rebuild a band summary (tests only).
#[cfg(test)]
pub(crate) fn band_scans_on_this_thread() -> usize {
    BAND_SCANS.with(std::cell::Cell::get)
}

/// A value stored at a cell coordinate.
pub trait StoredValue: Clone + Hash + Send + Sync {
    /// Whether the saved file records this value. Values that are not persisted
    /// (default styles and alignments) are left out of the content digest, as
    /// they are left out of the saved file.
    fn is_persisted(&self) -> bool {
        true
    }

    /// Whether the value is a formula.
    fn is_formula(&self) -> bool {
        false
    }

    /// Bytes the value owns beyond its inline size.
    fn heap_bytes(&self) -> usize {
        0
    }
}

/// Smallest rectangle containing every stored coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Extent {
    /// Smallest row index.
    pub min_row: u32,
    /// Largest row index.
    pub max_row: u32,
    /// Smallest column index.
    pub min_col: u32,
    /// Largest column index.
    pub max_col: u32,
}

impl Extent {
    fn point(at: CellRef) -> Self {
        Self {
            min_row: at.row,
            max_row: at.row,
            min_col: at.col,
            max_col: at.col,
        }
    }

    fn include(self, at: CellRef) -> Self {
        self.union(Self::point(at))
    }

    fn union(self, other: Self) -> Self {
        Self {
            min_row: self.min_row.min(other.min_row),
            max_row: self.max_row.max(other.max_row),
            min_col: self.min_col.min(other.min_col),
            max_col: self.max_col.max(other.max_col),
        }
    }
}

/// Order-independent fingerprint of the persisted entries of a map.
///
/// Maps with the same persisted entries have equal digests. Different content
/// can share a digest only when two independent 64-bit hashes collide at once,
/// which is not expected for user content.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Digest {
    count: usize,
    first: u64,
    second: u64,
}

impl Digest {
    fn combine(self, other: Self) -> Self {
        Self {
            count: self.count + other.count,
            first: self.first.wrapping_add(other.first),
            second: self.second.wrapping_add(other.second),
        }
    }
}

/// Two 64-bit fingerprints of one entry, from a single pass over its bytes.
fn fingerprint<V: Hash>(at: CellRef, value: &V) -> (u64, u64) {
    let mut hasher = DefaultHasher::new();
    at.hash(&mut hasher);
    value.hash(&mut hasher);
    let first = hasher.finish();
    hasher.write_u8(0x5a);
    (first, hasher.finish())
}

fn entry_bytes<V: StoredValue>(value: &V) -> usize {
    std::mem::size_of::<CellRef>()
        + std::mem::size_of::<V>()
        + ENTRY_OVERHEAD_BYTES
        + value.heap_bytes()
}

/// The entries of one row band with their summaries.
#[derive(Debug, Clone)]
struct Band<V> {
    entries: BTreeMap<CellRef, V>,
    extent: Option<Extent>,
    formulas: usize,
    formula_rows: BTreeMap<u32, usize>,
    bytes: usize,
    digest: OnceLock<Digest>,
}

impl<V: StoredValue> Band<V> {
    fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            extent: None,
            formulas: 0,
            formula_rows: BTreeMap::new(),
            bytes: 0,
            digest: OnceLock::new(),
        }
    }

    fn insert(&mut self, at: CellRef, value: V) -> Option<V> {
        self.digest.take();
        let formula = value.is_formula();
        self.bytes += entry_bytes(&value);
        let previous = self.entries.insert(at, value);
        match &previous {
            Some(old) => {
                self.bytes -= entry_bytes(old);
                if old.is_formula() {
                    self.drop_formula(at.row);
                }
            }
            None => {
                self.extent = Some(self.extent.map_or(Extent::point(at), |e| e.include(at)));
            }
        }
        if formula {
            self.add_formula(at.row);
        }
        previous
    }

    fn remove(&mut self, at: &CellRef) -> Option<V> {
        let removed = self.entries.remove(at)?;
        self.digest.take();
        self.bytes -= entry_bytes(&removed);
        if removed.is_formula() {
            self.drop_formula(at.row);
        }
        if let Some(extent) = self.extent {
            let on_edge = at.row == extent.min_row
                || at.row == extent.max_row
                || at.col == extent.min_col
                || at.col == extent.max_col;
            if on_edge {
                self.extent = self.rescan_extent();
            }
        }
        Some(removed)
    }

    fn add_formula(&mut self, row: u32) {
        self.formulas += 1;
        *self.formula_rows.entry(row).or_insert(0) += 1;
    }

    fn drop_formula(&mut self, row: u32) {
        self.formulas -= 1;
        if let Some(count) = self.formula_rows.get_mut(&row) {
            *count -= 1;
            if *count == 0 {
                self.formula_rows.remove(&row);
            }
        }
    }

    /// Rebuild the extent from the entries. Only needed when an edge entry leaves.
    fn rescan_extent(&self) -> Option<Extent> {
        #[cfg(test)]
        BAND_SCANS.with(|scans| scans.set(scans.get() + 1));
        self.entries.keys().copied().fold(None, |acc, at| {
            Some(acc.map_or(Extent::point(at), |e| e.include(at)))
        })
    }

    /// Rebuild every summary after values changed in place.
    fn recount(&mut self) {
        #[cfg(test)]
        BAND_SCANS.with(|scans| scans.set(scans.get() + 1));
        self.formulas = 0;
        self.formula_rows.clear();
        self.bytes = 0;
        for (at, value) in &self.entries {
            self.bytes += entry_bytes(value);
            if value.is_formula() {
                self.formulas += 1;
                *self.formula_rows.entry(at.row).or_insert(0) += 1;
            }
        }
    }

    fn digest(&self) -> Digest {
        *self.digest.get_or_init(|| {
            let mut digest = Digest::default();
            for (at, value) in &self.entries {
                if value.is_persisted() {
                    let (first, second) = fingerprint(*at, value);
                    digest = digest.combine(Digest {
                        count: 1,
                        first,
                        second,
                    });
                }
            }
            digest
        })
    }
}

/// Row-banded sparse map from cell coordinates to values.
///
/// Iteration visits entries in [`CellRef`] order, which is row-major because
/// bands partition rows in increasing order.
#[derive(Debug, Clone)]
pub struct BandedMap<V> {
    bands: BTreeMap<u32, Arc<Band<V>>>,
    len: usize,
}

impl<V> Default for BandedMap<V> {
    fn default() -> Self {
        Self {
            bands: BTreeMap::new(),
            len: 0,
        }
    }
}

fn band_key(at: CellRef) -> u32 {
    at.row / BAND_ROWS
}

impl<V: StoredValue> BandedMap<V> {
    /// An empty map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of stored entries.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the map stores nothing.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The value stored at a coordinate.
    pub fn get(&self, at: &CellRef) -> Option<&V> {
        self.bands.get(&band_key(*at))?.entries.get(at)
    }

    /// Whether a value is stored at a coordinate.
    pub fn contains_key(&self, at: &CellRef) -> bool {
        self.get(at).is_some()
    }

    /// Store a value, returning the one it replaced.
    pub fn insert(&mut self, at: CellRef, value: V) -> Option<V> {
        let band = self
            .bands
            .entry(band_key(at))
            .or_insert_with(|| Arc::new(Band::new()));
        let previous = writable(band).insert(at, value);
        if previous.is_none() {
            self.len += 1;
        }
        previous
    }

    /// Remove and return the value at a coordinate.
    pub fn remove(&mut self, at: &CellRef) -> Option<V> {
        let key = band_key(*at);
        let band = self.bands.get_mut(&key)?;
        if !band.entries.contains_key(at) {
            return None;
        }
        let removed = writable(band).remove(at);
        let emptied = band.entries.is_empty();
        if emptied {
            self.bands.remove(&key);
        }
        self.len -= 1;
        removed
    }

    /// Remove every entry.
    pub fn clear(&mut self) {
        self.bands.clear();
        self.len = 0;
    }

    /// Entries in coordinate order.
    pub fn iter(&self) -> Iter<'_, V> {
        Iter {
            bands: self.bands.values(),
            current: None,
        }
    }

    /// Coordinates in coordinate order.
    pub fn keys(&self) -> impl Iterator<Item = &CellRef> + '_ {
        self.iter().map(|(at, _)| at)
    }

    /// Values in coordinate order.
    pub fn values(&self) -> impl Iterator<Item = &V> + '_ {
        self.iter().map(|(_, value)| value)
    }

    /// Change every value in place. Summaries are rebuilt for each band once,
    /// after the loop, so the closure may change formula status freely.
    pub fn for_each_value_mut(&mut self, mut change: impl FnMut(&mut V)) {
        for band in self.bands.values_mut() {
            let band = writable(band);
            band.digest.take();
            for value in band.entries.values_mut() {
                change(value);
            }
            band.recount();
        }
    }

    /// Bounding box of every stored coordinate, in O(bands).
    pub fn extent(&self) -> Option<Extent> {
        self.bands
            .values()
            .filter_map(|band| band.extent)
            .reduce(Extent::union)
    }

    /// Number of formula entries, in O(bands).
    pub fn formula_count(&self) -> usize {
        self.bands.values().map(|band| band.formulas).sum()
    }

    /// The first row at or after `row` that holds a formula, in O(bands).
    pub fn first_formula_row_from(&self, row: u32) -> Option<u32> {
        self.bands
            .values()
            .filter_map(|band| {
                band.formula_rows
                    .range(row..)
                    .next()
                    .map(|(found, _)| *found)
            })
            .min()
    }

    /// Fingerprint of the persisted entries. Equal digests mean equal persisted
    /// content; each band's part is cached until the band changes.
    pub fn digest(&self) -> Digest {
        self.bands
            .values()
            .fold(Digest::default(), |acc, band| acc.combine(band.digest()))
    }

    /// Estimated bytes owned by this copy. Bands shared with other copies are
    /// counted here too; use [`Self::band_costs`] to count shared bands once.
    pub fn approximate_bytes(&self) -> usize {
        self.bands
            .values()
            .map(|band| BAND_OVERHEAD_BYTES + band.bytes)
            .sum()
    }

    /// Each band's identity and estimated bytes, so a caller holding several
    /// copies can count a shared band once.
    pub fn band_costs(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.bands
            .values()
            .map(|band| (Arc::as_ptr(band) as usize, BAND_OVERHEAD_BYTES + band.bytes))
    }

    /// Consume the map into its entries. A band nobody else shares gives up
    /// its values without copying them.
    pub fn into_entries(self) -> Vec<(CellRef, V)> {
        let mut entries = Vec::with_capacity(self.len);
        for band in self.bands.into_values() {
            match Arc::try_unwrap(band) {
                Ok(band) => entries.extend(band.entries),
                Err(shared) => entries.extend(
                    shared
                        .entries
                        .iter()
                        .map(|(at, value)| (*at, value.clone())),
                ),
            }
        }
        entries
    }
}

/// Make a shared band writable. A band still shared with another copy is
/// cloned here, which is the only place a write copies cell data.
fn writable<V: StoredValue>(band: &mut Arc<Band<V>>) -> &mut Band<V> {
    #[cfg(test)]
    if Arc::strong_count(band) > 1 {
        BAND_COPIES.with(|copies| copies.set(copies.get() + 1));
    }
    Arc::make_mut(band)
}

/// Entries of a [`BandedMap`] in coordinate order.
pub struct Iter<'a, V> {
    bands: btree_map::Values<'a, u32, Arc<Band<V>>>,
    current: Option<btree_map::Iter<'a, CellRef, V>>,
}

impl<'a, V> Iterator for Iter<'a, V> {
    type Item = (&'a CellRef, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(current) = self.current.as_mut() {
                if let Some(item) = current.next() {
                    return Some(item);
                }
            }
            let band = self.bands.next()?;
            self.current = Some(band.entries.iter());
        }
    }
}

impl<'a, V: StoredValue> IntoIterator for &'a BandedMap<V> {
    type Item = (&'a CellRef, &'a V);
    type IntoIter = Iter<'a, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<V: StoredValue> Extend<(CellRef, V)> for BandedMap<V> {
    fn extend<I: IntoIterator<Item = (CellRef, V)>>(&mut self, entries: I) {
        for (at, value) in entries {
            self.insert(at, value);
        }
    }
}

impl<V: StoredValue> FromIterator<(CellRef, V)> for BandedMap<V> {
    fn from_iter<I: IntoIterator<Item = (CellRef, V)>>(entries: I) -> Self {
        let mut map = Self::new();
        map.extend(entries);
        map
    }
}

#[cfg(test)]
#[path = "banded_tests.rs"]
mod tests;
