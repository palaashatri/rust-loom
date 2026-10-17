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
use std::sync::Arc;

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

/// Two 64-bit fingerprints of one entry at `col`, from a single pass over its bytes.
fn cell_fingerprint<V: Hash>(col: u32, value: &V) -> (u64, u64) {
    let mut hasher = DefaultHasher::new();
    col.hash(&mut hasher);
    value.hash(&mut hasher);
    let first = hasher.finish();
    hasher.write_u8(0x5a);
    (first, hasher.finish())
}

/// Odd 64-bit bijective mixer for a row index. Because multiplication distributes
/// over addition modulo 2^64, multiplying a row's summed cell fingerprints by its
/// row mixer gives an order-independent digest without rehashing cells when a
/// whole row moves to a new row index.
fn row_mixer(row: u32, seed: u64) -> u64 {
    let mut x = (row as u64).wrapping_add(seed);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    (x ^ (x >> 31)) | 1
}

fn entry_bytes<V: StoredValue>(value: &V) -> usize {
    std::mem::size_of::<CellRef>()
        + std::mem::size_of::<V>()
        + ENTRY_OVERHEAD_BYTES
        + value.heap_bytes()
}

/// One populated row inside a [`Band`]. Coordinates and values live in parallel
/// column-sorted `Arc<Vec<_>>` slices so moving a whole row to another row index
/// shares `values` without cloning cells.
#[derive(Debug, Clone)]
struct Row<V> {
    coords: Arc<Vec<CellRef>>,
    values: Arc<Vec<V>>,
    formulas: usize,
    bytes: usize,
    persisted_count: usize,
    sum_h1: u64,
    sum_h2: u64,
}

impl<V: StoredValue> Row<V> {
    fn new() -> Self {
        Self {
            coords: Arc::new(Vec::new()),
            values: Arc::new(Vec::new()),
            formulas: 0,
            bytes: 0,
            persisted_count: 0,
            sum_h1: 0,
            sum_h2: 0,
        }
    }

    fn len(&self) -> usize {
        self.coords.len()
    }

    fn is_empty(&self) -> bool {
        self.coords.is_empty()
    }

    fn extent(&self) -> Option<Extent> {
        let first = *self.coords.first()?;
        let last = *self.coords.last()?;
        Some(Extent {
            min_row: first.row,
            max_row: last.row,
            min_col: first.col,
            max_col: last.col,
        })
    }

    fn all_cols_within(&self, first_col: u32, last_col: u32) -> bool {
        match (self.coords.first(), self.coords.last()) {
            (Some(first), Some(last)) => first.col >= first_col && last.col <= last_col,
            _ => true,
        }
    }

    fn with_row_index(&self, row: u32) -> Self {
        if self.coords.first().map_or(true, |c| c.row == row) {
            return self.clone();
        }
        Self {
            coords: Arc::new(
                self.coords
                    .iter()
                    .map(|c| CellRef { row, col: c.col })
                    .collect(),
            ),
            values: Arc::clone(&self.values),
            formulas: self.formulas,
            bytes: self.bytes,
            persisted_count: self.persisted_count,
            sum_h1: self.sum_h1,
            sum_h2: self.sum_h2,
        }
    }

    fn get(&self, col: u32) -> Option<&V> {
        let idx = self.coords.binary_search_by_key(&col, |c| c.col).ok()?;
        self.values.get(idx)
    }

    fn add_entry_stats(&mut self, col: u32, value: &V) {
        self.bytes += entry_bytes(value);
        if value.is_formula() {
            self.formulas += 1;
        }
        if value.is_persisted() {
            let (h1, h2) = cell_fingerprint(col, value);
            self.persisted_count += 1;
            self.sum_h1 = self.sum_h1.wrapping_add(h1);
            self.sum_h2 = self.sum_h2.wrapping_add(h2);
        }
    }

    fn remove_entry_stats(&mut self, col: u32, value: &V) {
        self.bytes -= entry_bytes(value);
        if value.is_formula() {
            self.formulas -= 1;
        }
        if value.is_persisted() {
            let (h1, h2) = cell_fingerprint(col, value);
            self.persisted_count -= 1;
            self.sum_h1 = self.sum_h1.wrapping_sub(h1);
            self.sum_h2 = self.sum_h2.wrapping_sub(h2);
        }
    }

    fn insert(&mut self, at: CellRef, value: V) -> Option<V> {
        self.add_entry_stats(at.col, &value);
        match self.coords.binary_search_by_key(&at.col, |c| c.col) {
            Ok(idx) => {
                let previous = std::mem::replace(&mut Arc::make_mut(&mut self.values)[idx], value);
                self.remove_entry_stats(at.col, &previous);
                Some(previous)
            }
            Err(idx) => {
                Arc::make_mut(&mut self.coords).insert(idx, at);
                Arc::make_mut(&mut self.values).insert(idx, value);
                None
            }
        }
    }

    fn remove(&mut self, col: u32) -> Option<V> {
        let idx = self.coords.binary_search_by_key(&col, |c| c.col).ok()?;
        Arc::make_mut(&mut self.coords).remove(idx);
        let removed = Arc::make_mut(&mut self.values).remove(idx);
        self.remove_entry_stats(col, &removed);
        Some(removed)
    }

    fn recount(&mut self) {
        self.formulas = 0;
        self.bytes = 0;
        self.persisted_count = 0;
        self.sum_h1 = 0;
        self.sum_h2 = 0;
        for (at, value) in self.coords.iter().zip(self.values.iter()) {
            self.bytes += entry_bytes(value);
            if value.is_formula() {
                self.formulas += 1;
            }
            if value.is_persisted() {
                let (h1, h2) = cell_fingerprint(at.col, value);
                self.persisted_count += 1;
                self.sum_h1 = self.sum_h1.wrapping_add(h1);
                self.sum_h2 = self.sum_h2.wrapping_add(h2);
            }
        }
    }

    fn digest(&self, row: u32) -> Digest {
        if self.persisted_count == 0 {
            return Digest::default();
        }
        Digest {
            count: self.persisted_count,
            first: self
                .sum_h1
                .wrapping_mul(row_mixer(row, 0x9e37_79b9_7f4a_7c15)),
            second: self
                .sum_h2
                .wrapping_mul(row_mixer(row, 0x6a09_e667_f3bc_c908)),
        }
    }
}

/// The entries of one row band with their summaries.
#[derive(Debug, Clone)]
struct Band<V> {
    base_row: u32,
    rows: [Option<Row<V>>; BAND_ROWS as usize],
    extent: Option<Extent>,
    formulas: usize,
    bytes: usize,
    digest: Digest,
}

impl<V: StoredValue> Band<V> {
    fn new(base_row: u32) -> Self {
        Self {
            base_row,
            rows: std::array::from_fn(|_| None),
            extent: None,
            formulas: 0,
            bytes: 0,
            digest: Digest::default(),
        }
    }

    fn is_empty(&self) -> bool {
        self.rows.iter().all(Option::is_none)
    }

    fn refresh_totals(&mut self) {
        self.formulas = 0;
        self.bytes = 0;
        self.digest = Digest::default();
        for (offset, slot) in self.rows.iter().enumerate() {
            if let Some(row) = slot {
                let row_idx = self.base_row + offset as u32;
                self.formulas += row.formulas;
                self.bytes += row.bytes;
                self.digest = self.digest.combine(row.digest(row_idx));
            }
        }
    }

    fn insert(&mut self, at: CellRef, value: V) -> Option<V> {
        let slot = (at.row - self.base_row) as usize;
        let row = self.rows[slot].get_or_insert_with(Row::new);
        let previous = row.insert(at, value);
        if previous.is_none() {
            self.extent = Some(self.extent.map_or(Extent::point(at), |e| e.include(at)));
        }
        self.refresh_totals();
        previous
    }

    fn remove(&mut self, at: &CellRef) -> Option<V> {
        let slot = (at.row - self.base_row) as usize;
        let row = self.rows[slot].as_mut()?;
        let removed = row.remove(at.col)?;
        if row.is_empty() {
            self.rows[slot] = None;
        }
        self.refresh_totals();
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

    /// Rebuild the extent from the rows. Only needed when an edge entry leaves.
    fn rescan_extent(&self) -> Option<Extent> {
        #[cfg(test)]
        BAND_SCANS.with(|scans| scans.set(scans.get() + 1));
        self.rows
            .iter()
            .filter_map(|slot| slot.as_ref()?.extent())
            .reduce(Extent::union)
    }

    /// Rebuild every summary after values changed in place.
    fn recount(&mut self) {
        #[cfg(test)]
        BAND_SCANS.with(|scans| scans.set(scans.get() + 1));
        for row in self.rows.iter_mut().flatten() {
            row.recount();
        }
        self.refresh_totals();
    }

    fn digest(&self) -> Digest {
        self.digest
    }
}

/// Merge cells outside `first_col..=last_col` from `outside_row` with cells
/// inside `first_col..=last_col` from `inside_row`, all placed at `dest_row`.
fn merge_partial_row<V: StoredValue>(
    dest_row: u32,
    first_col: u32,
    last_col: u32,
    outside_row: Option<&Row<V>>,
    inside_row: Option<&Row<V>>,
) -> Option<Row<V>> {
    let mut merged = Row::new();
    if let Some(outside) = outside_row {
        for (at, value) in outside.coords.iter().zip(outside.values.iter()) {
            if at.col < first_col || at.col > last_col {
                merged.insert(
                    CellRef {
                        row: dest_row,
                        col: at.col,
                    },
                    value.clone(),
                );
            }
        }
    }
    if let Some(inside) = inside_row {
        for (at, value) in inside.coords.iter().zip(inside.values.iter()) {
            if at.col >= first_col && at.col <= last_col {
                merged.insert(
                    CellRef {
                        row: dest_row,
                        col: at.col,
                    },
                    value.clone(),
                );
            }
        }
    }
    (!merged.is_empty()).then_some(merged)
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
        let band = self.bands.get(&band_key(*at))?;
        let slot = (at.row - band.base_row) as usize;
        band.rows[slot].as_ref()?.get(at.col)
    }

    /// Whether a value is stored at a coordinate.
    pub fn contains_key(&self, at: &CellRef) -> bool {
        self.get(at).is_some()
    }

    /// Store a value, returning the one it replaced.
    pub fn insert(&mut self, at: CellRef, value: V) -> Option<V> {
        let key = band_key(at);
        let band = self
            .bands
            .entry(key)
            .or_insert_with(|| Arc::new(Band::new(key * BAND_ROWS)));
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
        let slot = (at.row - band.base_row) as usize;
        band.rows[slot].as_ref().and_then(|row| row.get(at.col))?;
        let removed = writable(band).remove(at);
        let emptied = band.is_empty();
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
            rows: [].iter(),
            coords: [].iter(),
            values: [].iter(),
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
            for row in band.rows.iter_mut().flatten() {
                for value in Arc::make_mut(&mut row.values).iter_mut() {
                    change(value);
                }
            }
            band.recount();
        }
    }

    /// Reorder the rows in `start_row .. start_row + source_offsets.len()` across
    /// columns `first_col..=last_col`, where destination row `start_row + d`
    /// receives the cells from `start_row + source_offsets[d]`.
    pub(crate) fn permute_rows(
        &mut self,
        start_row: u32,
        first_col: u32,
        last_col: u32,
        source_offsets: &[u32],
    ) {
        if source_offsets.is_empty() || self.is_empty() {
            return;
        }
        let end_row = start_row + source_offsets.len() as u32 - 1;
        let first_band = start_row / BAND_ROWS;
        let last_band = end_row / BAND_ROWS;
        let old_bands = self.bands.clone();
        let row_at = |row: u32| -> Option<&Row<V>> {
            let band = old_bands.get(&(row / BAND_ROWS))?;
            band.rows[(row % BAND_ROWS) as usize].as_ref()
        };

        let mut new_len = self.len;
        for band_idx in first_band..=last_band {
            let base_row = band_idx * BAND_ROWS;
            if let Some(existing) = old_bands.get(&band_idx) {
                let old_band_entries: usize = existing.rows.iter().flatten().map(Row::len).sum();
                new_len -= old_band_entries;
            }
            let mut band = Band::new(base_row);
            let mut band_entries = 0usize;
            for slot in 0..BAND_ROWS as usize {
                let dest_row = base_row + slot as u32;
                let next_row = if dest_row < start_row || dest_row > end_row {
                    row_at(dest_row).cloned()
                } else {
                    let src_row = start_row + source_offsets[(dest_row - start_row) as usize];
                    let dest_existing = row_at(dest_row);
                    let src_existing = row_at(src_row);
                    let whole_row = dest_existing
                        .map_or(true, |r| r.all_cols_within(first_col, last_col))
                        && src_existing.map_or(true, |r| r.all_cols_within(first_col, last_col));
                    if whole_row {
                        src_existing.map(|r| r.with_row_index(dest_row))
                    } else {
                        merge_partial_row(
                            dest_row,
                            first_col,
                            last_col,
                            dest_existing,
                            src_existing,
                        )
                    }
                };
                if let Some(row) = &next_row {
                    band_entries += row.len();
                }
                band.rows[slot] = next_row;
            }
            new_len += band_entries;
            if band.is_empty() {
                self.bands.remove(&band_idx);
            } else {
                band.extent = band
                    .rows
                    .iter()
                    .filter_map(|slot| slot.as_ref()?.extent())
                    .reduce(Extent::union);
                band.refresh_totals();
                self.bands.insert(band_idx, Arc::new(band));
            }
        }
        self.len = new_len;
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
        let start_band = row / BAND_ROWS;
        for (&band_idx, band) in self.bands.range(start_band..) {
            if band.formulas == 0 {
                continue;
            }
            let first_slot = if band_idx == start_band {
                (row % BAND_ROWS) as usize
            } else {
                0
            };
            for (offset, slot) in band.rows[first_slot..].iter().enumerate() {
                if slot.as_ref().is_some_and(|r| r.formulas > 0) {
                    return Some(band_idx * BAND_ROWS + (first_slot + offset) as u32);
                }
            }
        }
        None
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
                Ok(band) => {
                    for row in band.rows.into_iter().flatten() {
                        match (Arc::try_unwrap(row.coords), Arc::try_unwrap(row.values)) {
                            (Ok(coords), Ok(values)) => {
                                entries.extend(coords.into_iter().zip(values));
                            }
                            (coords, values) => {
                                let coords = coords.unwrap_or_else(|shared| (*shared).clone());
                                let values = values.unwrap_or_else(|shared| (*shared).clone());
                                entries.extend(coords.into_iter().zip(values));
                            }
                        }
                    }
                }
                Err(shared) => {
                    for row in shared.rows.iter().flatten() {
                        entries.extend(row.coords.iter().copied().zip(row.values.iter().cloned()));
                    }
                }
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
    rows: std::slice::Iter<'a, Option<Row<V>>>,
    coords: std::slice::Iter<'a, CellRef>,
    values: std::slice::Iter<'a, V>,
}

impl<'a, V> Iterator for Iter<'a, V> {
    type Item = (&'a CellRef, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let (Some(at), Some(val)) = (self.coords.next(), self.values.next()) {
                return Some((at, val));
            }
            for row in self.rows.by_ref() {
                if let Some(row) = row.as_ref() {
                    self.coords = row.coords.iter();
                    self.values = row.values.iter();
                    if let (Some(at), Some(val)) = (self.coords.next(), self.values.next()) {
                        return Some((at, val));
                    }
                }
            }
            let band = self.bands.next()?;
            self.rows = band.rows.iter();
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
