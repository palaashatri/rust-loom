use super::*;
use crate::{Cell, CellAlignment};
use std::collections::BTreeMap;

fn at(row: u32, col: u32) -> CellRef {
    CellRef { row, col }
}

fn cell(raw: &str) -> Cell {
    Cell {
        raw: raw.to_string(),
    }
}

/// Distinct coordinates spread over several bands, in a fixed scrambled order.
fn scattered(count: u32) -> Vec<CellRef> {
    (0..count)
        .map(|index| {
            let scrambled = index.wrapping_mul(2_654_435_761) % (count * 3);
            at(scrambled / 40, scrambled % 40)
        })
        .collect()
}

#[test]
fn an_empty_map_has_no_extent_formulas_or_bytes() {
    let map: BandedMap<Cell> = BandedMap::new();
    assert!(map.is_empty());
    assert_eq!(map.len(), 0);
    assert_eq!(map.extent(), None);
    assert_eq!(map.formula_count(), 0);
    assert_eq!(map.first_formula_row_from(0), None);
    assert_eq!(map.approximate_bytes(), 0);
}

#[test]
fn iteration_follows_coordinate_order_across_bands() {
    let mut map = BandedMap::new();
    let mut reference = BTreeMap::new();
    for (index, coordinate) in scattered(1_500).into_iter().enumerate() {
        let value = cell(&format!("v{index}"));
        assert_eq!(
            map.insert(coordinate, value.clone()),
            reference.insert(coordinate, value)
        );
    }
    assert_eq!(map.len(), reference.len());
    let ours: Vec<(CellRef, Cell)> = map.iter().map(|(at, value)| (*at, value.clone())).collect();
    let theirs: Vec<(CellRef, Cell)> = reference.into_iter().collect();
    assert_eq!(ours, theirs);
}

#[test]
fn removals_and_replacements_agree_with_a_btreemap_reference() {
    let mut map = BandedMap::new();
    let mut reference = BTreeMap::new();
    for (index, coordinate) in scattered(900).into_iter().enumerate() {
        if index % 3 == 2 {
            assert_eq!(map.remove(&coordinate), reference.remove(&coordinate));
        } else {
            let value = cell(&format!("={index}"));
            assert_eq!(
                map.insert(coordinate, value.clone()),
                reference.insert(coordinate, value)
            );
        }
        assert_eq!(map.len(), reference.len());
    }
    for (coordinate, value) in &reference {
        assert_eq!(map.get(coordinate), Some(value));
    }
    assert_eq!(map.keys().count(), reference.len());
}

#[test]
fn a_clone_shares_every_band_and_a_write_copies_only_the_band_it_touches() {
    let mut map = BandedMap::new();
    for row in 0..400 {
        map.insert(at(row, 0), cell("1"));
    }
    let before_clone = band_copies_on_this_thread();
    let snapshot = map.clone();
    assert_eq!(band_copies_on_this_thread(), before_clone);

    let before_write = band_copies_on_this_thread();
    map.insert(at(5, 1), cell("2"));
    assert_eq!(band_copies_on_this_thread() - before_write, 1);
    map.insert(at(6, 1), cell("3"));
    assert_eq!(band_copies_on_this_thread() - before_write, 1);

    assert_eq!(snapshot.get(&at(5, 1)), None);
    assert_eq!(map.get(&at(5, 1)).map(|c| c.raw.as_str()), Some("2"));
    assert_eq!(snapshot.len(), 400);
    assert_eq!(map.len(), 402);
}

#[test]
fn extent_tracks_edges_and_interior_removals_need_no_rescan() {
    let mut map = BandedMap::new();
    for row in 0..50 {
        for col in 0..10 {
            map.insert(at(row, col), cell("x"));
        }
    }
    assert_eq!(
        map.extent(),
        Some(Extent {
            min_row: 0,
            max_row: 49,
            min_col: 0,
            max_col: 9,
        })
    );
    let scans = band_scans_on_this_thread();
    map.remove(&at(20, 5));
    map.insert(at(20, 5), cell("y"));
    assert_eq!(band_scans_on_this_thread(), scans);

    // Removing the whole last row moves the bottom edge up; that edge removal
    // is the one case that rescans the band.
    let scans = band_scans_on_this_thread();
    for col in 0..10 {
        map.remove(&at(49, col));
    }
    assert!(band_scans_on_this_thread() > scans);
    assert_eq!(
        map.extent(),
        Some(Extent {
            min_row: 0,
            max_row: 48,
            min_col: 0,
            max_col: 9,
        })
    );
}

#[test]
fn formula_count_and_first_formula_row_follow_inserts_removals_and_rewrites() {
    let mut map = BandedMap::new();
    map.insert(at(0, 0), cell("=A1"));
    map.insert(at(3, 0), cell("7"));
    map.insert(at(5, 2), cell("=B1+1"));
    map.insert(at(40, 1), cell("=C2"));
    assert_eq!(map.formula_count(), 3);
    assert_eq!(map.first_formula_row_from(0), Some(0));
    assert_eq!(map.first_formula_row_from(1), Some(5));

    map.remove(&at(5, 2));
    assert_eq!(map.first_formula_row_from(1), Some(40));

    map.insert(at(40, 1), cell("text"));
    assert_eq!(map.formula_count(), 1);
    assert_eq!(map.first_formula_row_from(1), None);

    map.for_each_value_mut(|value| {
        if value.raw == "7" {
            value.raw = "=D1".to_string();
        }
    });
    assert_eq!(map.formula_count(), 2);
    assert_eq!(map.first_formula_row_from(1), Some(3));
}

#[test]
fn digest_depends_only_on_persisted_content_and_returns_after_an_undo() {
    let mut forward = BandedMap::new();
    let mut backward = BandedMap::new();
    let coordinates: Vec<CellRef> = (0..500).map(|i| at(i / 100, i % 100)).collect();
    for (index, coordinate) in coordinates.iter().enumerate() {
        forward.insert(*coordinate, cell(&index.to_string()));
    }
    for (index, coordinate) in coordinates.iter().enumerate().rev() {
        backward.insert(*coordinate, cell(&index.to_string()));
    }
    assert_eq!(forward.digest(), backward.digest());

    let original = backward.get(&at(0, 0)).cloned().expect("cell A1");
    backward.insert(at(0, 0), cell("changed"));
    assert_ne!(forward.digest(), backward.digest());
    backward.insert(at(0, 0), original);
    assert_eq!(forward.digest(), backward.digest());
}

#[test]
fn values_that_are_not_persisted_leave_the_digest_unchanged() {
    let mut alignments: BandedMap<CellAlignment> = BandedMap::new();
    let empty = alignments.digest();
    alignments.insert(at(0, 0), CellAlignment::General);
    assert_eq!(alignments.digest(), empty);
    alignments.insert(at(0, 0), CellAlignment::Center);
    assert_ne!(alignments.digest(), empty);
}

#[test]
fn byte_estimates_grow_with_content_and_shared_bands_can_be_identified() {
    let mut map = BandedMap::new();
    for row in 0..64 {
        map.insert(at(row, 0), cell("abc"));
    }
    let before = map.approximate_bytes();
    assert!(before >= 64 * std::mem::size_of::<Cell>());
    let snapshot = map.clone();
    let shared: Vec<usize> = snapshot.band_costs().map(|(id, _)| id).collect();
    let original: Vec<usize> = map.band_costs().map(|(id, _)| id).collect();
    assert_eq!(original, shared);

    map.insert(at(0, 0), cell("changed"));
    let after: Vec<usize> = map.band_costs().map(|(id, _)| id).collect();
    assert_ne!(after[0], shared[0]);
    assert_eq!(after[1..], shared[1..]);
    assert_eq!(snapshot.approximate_bytes(), before);
}

#[test]
fn clear_and_collect_round_trip() {
    let entries: Vec<(CellRef, Cell)> = (0..40).map(|i| (at(i, i % 3), cell("1"))).collect();
    let mut map: BandedMap<Cell> = entries.clone().into_iter().collect();
    assert_eq!(map.len(), 40);
    assert_eq!(map.extent().map(|e| e.max_row), Some(39));
    map.clear();
    assert!(map.is_empty());
    assert_eq!(map.extent(), None);
}
