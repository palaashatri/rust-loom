//! Bounded SVG projection for embedded Writer charts.
//!
//! The chart object remains the source of truth. This module only projects its
//! current values into the same image-tile path used for shaped document text.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::{Arc, Weak};

use loom_objects::chart::{Chart, ChartType};
use loom_writer_core::RichBlock;

#[path = "chart_projection/cartesian.rs"]
mod cartesian;
#[path = "chart_projection/pie.rs"]
mod pie;
#[path = "chart_projection/primitives.rs"]
mod primitives;
#[cfg(test)]
#[path = "chart_projection_tests.rs"]
mod tests;

const MAX_CATEGORIES: usize = 20;
const MAX_SERIES: usize = 8;
const MAX_LABEL_CHARS: usize = 48;
const MAX_CACHED_CHART_JSON_BYTES: usize = 256 * 1024;
const IMAGE_CACHE_SIZE: usize = 8;
const CHART_CACHE_SIZE: usize = 8;

const PALETTE: [&str; 8] = [
    "#4472C4", "#ED7D31", "#A5A5A5", "#FFC000", "#5B9BD5", "#70AD47", "#264478", "#9E480E",
];

thread_local! {
    /// Parsed chart models and image resources are retained only in small LRUs.
    static CHART_CACHE: RefCell<VecDeque<CachedChart>> =
        const { RefCell::new(VecDeque::new()) };
    static IMAGE_CACHE: RefCell<VecDeque<CachedImage>> =
        const { RefCell::new(VecDeque::new()) };
}

struct CachedChart {
    source: Weak<String>,
    chart: Arc<Chart>,
}

struct CachedImage {
    chart: Weak<Chart>,
    source: Option<Weak<String>>,
    width_bits: u32,
    height_bits: u32,
    image: slint::Image,
}

/// Build a bounded plot for a chart. Oversized charts show the leading
/// categories and series with an explicit note; malformed visible data returns
/// an error that the caller renders as text.
pub(super) fn svg(chart: &Chart, width: f32, height: f32) -> Result<String, String> {
    if !width.is_finite() || !height.is_finite() || width < 150.0 || height < 100.0 {
        return Err("the chart area is too small to plot".to_string());
    }
    if chart.categories.is_empty() {
        return Err("the chart has no categories".to_string());
    }
    if chart.series.is_empty() {
        return Err("the chart has no data series".to_string());
    }

    let category_count = chart.categories.len().min(MAX_CATEGORIES);
    let series_count = chart.series.len().min(MAX_SERIES);
    for series in chart.series.iter().take(series_count) {
        for value in series.values.iter().take(category_count) {
            if !value.is_finite() {
                return Err("a series contains a non-finite value".to_string());
            }
            if value.abs() > 1.0e100 {
                return Err("a series contains a value too large to plot".to_string());
            }
        }
    }
    if chart.chart_type == ChartType::Pie {
        for series in chart.series.iter().take(series_count) {
            if series
                .values
                .iter()
                .take(category_count)
                .any(|value| *value < 0.0)
            {
                return Err("pie charts require non-negative values".to_string());
            }
            if series
                .values
                .iter()
                .take(category_count)
                .all(|value| *value == 0.0)
            {
                return Err("pie series must contain a positive value".to_string());
            }
        }
    }

    let scatter_axis = cartesian::scatter_axis(chart, category_count)?;
    let mut out = String::with_capacity(12_000);
    write!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width:.2}" height="{height:.2}" viewBox="0 0 {width:.2} {height:.2}">"#
    )
    .expect("writing to a string cannot fail");
    let title = chart
        .title
        .as_deref()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or("Chart");
    let center_x = f64::from(width) / 2.0;
    primitives::chart_title(
        &mut out,
        center_x,
        13.0,
        &primitives::short_label(title, 56),
    );

    let limited_categories = chart.categories.len() > MAX_CATEGORIES;
    let limited_series = chart.series.len() > MAX_SERIES;
    let unmatched_values = chart
        .series
        .iter()
        .take(series_count)
        .any(|series| series.values.len() > chart.categories.len());
    if limited_categories || limited_series || unmatched_values {
        let mut note = format!(
            "Showing {} of {} categories and {} of {} series",
            category_count,
            chart.categories.len(),
            series_count,
            chart.series.len()
        );
        if unmatched_values {
            note.push_str("; extra values without categories omitted");
        }
        primitives::text(
            &mut out,
            (center_x, 25.0),
            6.2,
            "#666666",
            "middle",
            &note,
            false,
        );
    } else if chart.chart_type == ChartType::Pie {
        if let Some(unit) = chart
            .value_axis
            .title
            .as_deref()
            .filter(|unit| !unit.trim().is_empty())
        {
            primitives::text(
                &mut out,
                (center_x, 25.0),
                7.0,
                "#555555",
                "middle",
                unit,
                false,
            );
        }
    }

    if chart.chart_type == ChartType::Pie {
        pie::render(&mut out, chart, width, height, category_count, series_count);
    } else {
        cartesian::render(
            &mut out,
            chart,
            width,
            height,
            category_count,
            series_count,
            scatter_axis,
        );
    }

    out.push_str("</svg>");
    Ok(out)
}

/// Reuse a parsed chart while its immutable JSON source is alive. Large
/// payloads bypass this cache so a handful of snapshots cannot dominate memory.
pub(super) fn model(block: &RichBlock) -> Option<Arc<Chart>> {
    if block.kind != loom_writer_core::CHART_BLOCK_KIND {
        return None;
    }
    let source = block.chart.as_ref()?;
    let cacheable = source.len() <= MAX_CACHED_CHART_JSON_BYTES;
    if cacheable {
        if let Some(chart) = CHART_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            cache.retain(|cached| cached.source.strong_count() > 0);
            let index = cache.iter().position(|cached| {
                cached
                    .source
                    .upgrade()
                    .is_some_and(|cached_source| Arc::ptr_eq(&cached_source, source))
            })?;
            let entry = cache.remove(index)?;
            let chart = Arc::clone(&entry.chart);
            cache.push_front(entry);
            Some(chart)
        }) {
            return Some(chart);
        }
    }

    let chart = Arc::new(Chart::from_json(source.as_str()).ok()?);
    if cacheable {
        CHART_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            cache.push_front(CachedChart {
                source: Arc::downgrade(source),
                chart: Arc::clone(&chart),
            });
            cache.truncate(CHART_CACHE_SIZE);
        });
    }
    Some(chart)
}

/// Reuse an image by typed immutable Chart identity and exact plot dimensions
/// before generating SVG or asking Slint to parse it.
pub(super) fn image(chart: &Arc<Chart>, width: f32, height: f32) -> Result<slint::Image, String> {
    let width_bits = width.to_bits();
    let height_bits = height.to_bits();
    if let Some(image) = IMAGE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let index = cache.iter().position(|cached| {
            cached
                .chart
                .upgrade()
                .is_some_and(|cached_chart| Arc::ptr_eq(&cached_chart, chart))
                && cached.width_bits == width_bits
                && cached.height_bits == height_bits
        })?;
        let entry = cache.remove(index)?;
        let image = entry.image.clone();
        cache.push_front(entry);
        Some(image)
    }) {
        return Ok(image);
    }

    let svg_source = svg(chart, width, height)?;
    let image = slint::Image::load_from_svg_data(svg_source.as_bytes())
        .map_err(|_| "the chart graphic could not be rendered".to_string())?;
    IMAGE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.push_front(CachedImage {
            chart: Arc::downgrade(chart),
            source: None,
            width_bits,
            height_bits,
            image: image.clone(),
        });
        cache.truncate(IMAGE_CACHE_SIZE);
    });
    Ok(image)
}

/// Projects a chart block and caches its raster by the immutable document JSON
/// identity. This also lets large chart payloads reuse the bounded image cache
/// without retaining a parsed `Chart` for every history snapshot.
pub(super) fn image_for_block(
    block: &RichBlock,
    width: f32,
    height: f32,
) -> Result<slint::Image, String> {
    let source = block
        .chart
        .as_ref()
        .ok_or_else(|| "chart data is invalid".to_string())?;
    let width_bits = width.to_bits();
    let height_bits = height.to_bits();
    if let Some(image) = IMAGE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let index = cache.iter().position(|cached| {
            cached.source.as_ref().is_some_and(|cached_source| {
                cached_source
                    .upgrade()
                    .is_some_and(|cached_source| Arc::ptr_eq(&cached_source, source))
            }) && cached.width_bits == width_bits
                && cached.height_bits == height_bits
        })?;
        let entry = cache.remove(index)?;
        let image = entry.image.clone();
        cache.push_front(entry);
        Some(image)
    }) {
        return Ok(image);
    }

    let chart = model(block).ok_or_else(|| "chart data is invalid".to_string())?;
    let image = image(&chart, width, height)?;
    IMAGE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(entry) = cache.iter_mut().find(|cached| {
            cached
                .chart
                .upgrade()
                .is_some_and(|cached_chart| Arc::ptr_eq(&cached_chart, &chart))
                && cached.width_bits == width_bits
                && cached.height_bits == height_bits
        }) {
            entry.source = Some(Arc::downgrade(source));
        }
    });
    Ok(image)
}
