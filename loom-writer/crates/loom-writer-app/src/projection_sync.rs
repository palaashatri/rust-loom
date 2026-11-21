//! In-place Slint `VecModel` synchronization for Writer projections.
//!
//! Replacing outer `ModelRc` instances on every keystroke rebuilds every
//! materialized repeater row in Slint even when only one cell or paragraph
//! changed. Updating `VecModel` items in place keeps table typing fast.

use crate::{WriterApp, WriterGlyphTile, WriterRenderBlock};
use slint::{Model, ModelRc, VecModel};
use std::rc::Rc;

fn sync_vec_model_with<T: Clone + 'static>(
    model: &VecModel<T>,
    next: Vec<T>,
    eq: fn(&T, &T) -> bool,
) {
    let old_len = model.row_count();
    let new_len = next.len();
    let common = old_len.min(new_len);
    let mut iter = next.into_iter();
    for i in 0..common {
        let item = iter.next().expect("common <= next.len()");
        let unchanged = model
            .row_data(i)
            .is_some_and(|existing| eq(&existing, &item));
        if !unchanged {
            model.set_row_data(i, item);
        }
    }
    if new_len > old_len {
        for item in iter {
            model.push(item);
        }
    } else if old_len > new_len {
        for i in (new_len..old_len).rev() {
            model.remove(i);
        }
    }
}

/// Updates an existing `VecModel` in place when possible, replacing the outer
/// `ModelRc` only on first initialization or if backed by another model type.
pub(crate) fn sync_vec_model<T: PartialEq + Clone + 'static>(
    current: &ModelRc<T>,
    next: Vec<T>,
    setter: impl FnOnce(ModelRc<T>),
) {
    if let Some(model) = current.as_any().downcast_ref::<VecModel<T>>() {
        sync_vec_model_with(model, next, |a, b| a == b);
    } else {
        setter(Rc::new(VecModel::from(next)).into());
    }
}

fn model_eq<T>(a: &ModelRc<T>, b: &ModelRc<T>, eq: fn(&T, &T) -> bool) -> bool {
    let count = a.row_count();
    if count != b.row_count() {
        return false;
    }
    (0..count).all(|i| match (a.row_data(i), b.row_data(i)) {
        (Some(x), Some(y)) => eq(&x, &y),
        _ => false,
    })
}

fn glyph_tile_eq(a: &WriterGlyphTile, b: &WriterGlyphTile) -> bool {
    a.x == b.x && a.y == b.y && a.width == b.width && a.height == b.height && a.source == b.source
}

fn render_block_eq(a: &WriterRenderBlock, b: &WriterRenderBlock) -> bool {
    a.positioned == b.positioned
        && a.x == b.x
        && a.y == b.y
        && a.width == b.width
        && a.height == b.height
        && a.font_size == b.font_size
        && a.font_family == b.font_family
        && a.alignment == b.alignment
        && a.marker == b.marker
        && a.unsupported == b.unsupported
        && a.unsupported_label == b.unsupported_label
        && a.has_color == b.has_color
        && a.color == b.color
        && a.table_cell == b.table_cell
        && model_eq(&a.segments, &b.segments, |x, y| x == y)
        && model_eq(&a.rules, &b.rules, |x, y| x == y)
        && model_eq(&a.glyph_tiles, &b.glyph_tiles, glyph_tile_eq)
}

/// Updates `WriterApp::render_blocks` in place so unchanged rows (including
/// untouched table cells) keep their existing Slint repeater items.
pub(crate) fn sync_render_blocks(app: &WriterApp, rows: Vec<WriterRenderBlock>) {
    let current = app.get_render_blocks();
    if let Some(model) = current
        .as_any()
        .downcast_ref::<VecModel<WriterRenderBlock>>()
    {
        sync_vec_model_with(model, rows, render_block_eq);
    } else {
        app.set_render_blocks(Rc::new(VecModel::from(rows)).into());
    }
}
