use std::collections::HashMap;
use std::time::Instant;

use loom_sheets_core::{CellRef, Sheet, SheetDimensions, SheetViewport, Value};

use super::project_sheet_grid_with_values;

/// Opt-in CPU-side virtualization check. This measures building the visible
/// cell projection, not native window painting or frame presentation.
#[test]
fn million_sparse_cells_project_one_viewport_within_one_frame() {
    if std::env::var_os("LOOM_ENFORCE_SCROLL_BUDGET").is_none() {
        return;
    }

    const CELLS: u64 = 1_000_000;
    const SIDE: u32 = 2_048;
    const MAX_SCROLL_X: u32 = SIDE * 80 - 1_024;
    const MAX_SCROLL_Y: u32 = SIDE * 24 - 720;
    let mut sheet = Sheet::new("Million sparse cells");
    let mut quadrants = [0usize; 4];
    for index in 0..CELLS {
        // A keyed Feistel permutation gives unique, reproducible addresses
        // with a random-looking spread and no extra million-entry index.
        let address = pseudorandom_address(index as u32);
        let row = address / SIDE;
        let col = address % SIDE;
        let quadrant = usize::from(row >= SIDE / 2) * 2 + usize::from(col >= SIDE / 2);
        quadrants[quadrant] += 1;
        sheet.set_raw(CellRef { row, col }, "42");
    }
    assert_eq!(sheet.cells.len(), CELLS as usize);
    for count in quadrants {
        assert!(
            (200_000..300_000).contains(&count),
            "pseudorandom fixture is clustered: quadrant has {count} cells"
        );
    }

    let dimensions = SheetDimensions::new(SIDE, SIDE);
    let values = HashMap::<CellRef, Value>::new();
    let mut frame_times = Vec::with_capacity(60);
    for frame in 0..60u32 {
        let (scroll_x, scroll_y) = match frame {
            0 => (0.0, 0.0),
            59 => (1_000_000.0, 1_000_000.0),
            _ => (
                (pseudorandom_address(frame * 65_537) % MAX_SCROLL_X) as f32,
                (pseudorandom_address(frame * 83 + 1) % MAX_SCROLL_Y) as f32,
            ),
        };
        let viewport =
            SheetViewport::from_scroll(scroll_x, scroll_y, 1_024.0, 720.0, 24.0, 80.0, dimensions);
        if frame == 0 {
            assert_eq!((viewport.first_row, viewport.first_col), (0, 0));
        }
        if frame == 59 {
            assert_eq!(viewport.first_row + viewport.visible_rows, SIDE);
            assert_eq!(viewport.first_col + viewport.visible_cols, SIDE);
        }

        let started = Instant::now();
        let projected = project_sheet_grid_with_values(&sheet, &values, viewport);
        frame_times.push(started.elapsed());
        assert_eq!(
            projected.cells.len(),
            (viewport.visible_rows * viewport.visible_cols) as usize
        );
    }

    frame_times.sort_unstable();
    let p95_ms = frame_times[56].as_secs_f64() * 1_000.0;
    let max_ms = frame_times[59].as_secs_f64() * 1_000.0;
    eprintln!(
        "PERF scroll_projection cells={CELLS} frames=60 p95_ms={p95_ms:.2} max_ms={max_ms:.2}"
    );
    assert!(
        max_ms < 16.7,
        "visible-cell projection exceeded the 16.7 ms frame budget: {max_ms:.2} ms"
    );
}

fn pseudorandom_address(index: u32) -> u32 {
    const HALF_MASK: u32 = (1 << 11) - 1;
    const ROUND_KEYS: [u32; 6] = [0x12d, 0x3a7, 0x55b, 0x1e3, 0x6c1, 0x2f9];

    let mut left = (index >> 11) & HALF_MASK;
    let mut right = index & HALF_MASK;
    for key in ROUND_KEYS {
        let mut mixed = right ^ key;
        mixed = mixed.wrapping_mul(0x045d_9f3b);
        mixed ^= mixed >> 11;
        mixed = mixed.wrapping_mul(0x045d_9f3b);
        mixed ^= mixed >> 7;
        let next_right = left ^ (mixed & HALF_MASK);
        left = right;
        right = next_right;
    }
    (left << 11) | right
}
