//! Focused coverage for Writer's chart projection.

use super::*;
use loom_objects::chart::ChartSeries;
use std::sync::Arc;

fn sample(chart_type: ChartType) -> Chart {
    let categories = if chart_type == ChartType::Scatter {
        vec!["1".to_string(), "2".to_string(), "3".to_string()]
    } else {
        vec!["Q1 & start".to_string(), "Q2".to_string(), "Q3".to_string()]
    };
    let mut chart = Chart::from_data(
        "chart-1",
        Some("Quarterly <Revenue>".to_string()),
        chart_type,
        categories,
        vec![
            ChartSeries {
                name: "North <team>".to_string(),
                values: vec![3.0, 7.5, 5.0],
                color: Some("#123456".to_string()),
            },
            ChartSeries {
                name: "South".to_string(),
                values: vec![2.0, 4.0, 6.0],
                color: None,
            },
        ],
    );
    chart.value_axis.title = Some("€ / unit".to_string());
    chart.category_axis.title = Some("Quarter".to_string());
    chart.style.show_legend = true;
    chart.style.show_data_labels = true;
    chart
}

#[test]
fn every_chart_type_projects_marks_and_escaped_data_labels() {
    for chart_type in [
        ChartType::Column,
        ChartType::Bar,
        ChartType::Line,
        ChartType::Pie,
        ChartType::Area,
        ChartType::Scatter,
    ] {
        let chart = Arc::new(sample(chart_type));
        let output =
            svg(&chart, 400.0, 184.0).unwrap_or_else(|error| panic!("{chart_type:?}: {error}"));
        assert!(
            output.contains("Quarterly &lt;Revenue&gt;"),
            "{chart_type:?}"
        );
        if chart_type == ChartType::Scatter {
            assert!(!output.contains("Q1 &amp; start"));
        } else {
            assert!(output.contains("Q1 &amp; start"), "{chart_type:?}");
        }
        assert!(output.contains("North &lt;team&gt;"), "{chart_type:?}");
        assert!(output.contains("€ / unit"), "{chart_type:?}");
        assert!(output.contains("<rect") || output.contains("<circle") || output.contains("<path"));
        image(&chart, 400.0, 184.0).unwrap_or_else(|error| panic!("{chart_type:?}: {error}"));
        match chart_type {
            ChartType::Column | ChartType::Bar => assert!(output.contains("<rect")),
            ChartType::Line | ChartType::Area => assert!(output.contains("<path")),
            ChartType::Pie => assert!(output.contains(" A ") || output.contains("<circle")),
            ChartType::Scatter => assert!(output.contains("<circle")),
        }
    }
}

#[test]
fn changing_chart_values_changes_the_plotted_output_for_every_type() {
    for chart_type in [
        ChartType::Column,
        ChartType::Bar,
        ChartType::Line,
        ChartType::Pie,
        ChartType::Area,
        ChartType::Scatter,
    ] {
        let mut chart = sample(chart_type);
        chart.style.show_legend = false;
        chart.style.show_data_labels = false;
        let before =
            svg(&chart, 400.0, 184.0).unwrap_or_else(|error| panic!("{chart_type:?}: {error}"));

        chart.series[0].values[1] = 35.0;
        let after =
            svg(&chart, 400.0, 184.0).unwrap_or_else(|error| panic!("{chart_type:?}: {error}"));

        assert_ne!(
            before, after,
            "{chart_type:?} must plot the supplied values"
        );
        assert!(
            after.contains("<rect") || after.contains("<circle") || after.contains("<path"),
            "{chart_type:?} must produce plotted marks"
        );
    }
}

#[test]
fn scatter_plot_uses_numeric_x_positions() {
    let mut chart = sample(ChartType::Scatter);
    chart.categories = vec!["0".into(), "1".into(), "10".into()];
    chart.series.truncate(1);
    chart.style.show_legend = false;
    chart.style.show_data_labels = false;

    let output = svg(&chart, 400.0, 184.0).expect("numeric scatter X values plot");
    let positions: Vec<f64> = output
        .split("<circle cx=\"")
        .skip(1)
        .filter_map(|tail| tail.split('"').next()?.parse().ok())
        .collect();

    assert_eq!(positions.len(), 3);
    assert!(
        positions[2] - positions[1] > (positions[1] - positions[0]) * 4.0,
        "numeric X values 0, 1, and 10 must not be spaced evenly: {positions:?}"
    );
}

#[test]
fn scatter_plot_reports_non_numeric_x_values() {
    let mut chart = sample(ChartType::Scatter);
    chart.categories[1] = "not a number".into();

    assert!(svg(&chart, 400.0, 184.0)
        .unwrap_err()
        .contains("numeric X values"));
}

#[test]
fn bounded_projection_reports_hidden_categories_and_series() {
    let mut chart = sample(ChartType::Column);
    chart.categories = (0..25).map(|index| format!("Category {index}")).collect();
    chart.series = (0..10)
        .map(|index| ChartSeries {
            name: format!("Series {index}"),
            values: vec![index as f64; 25],
            color: None,
        })
        .collect();
    let output = svg(&chart, 400.0, 184.0).expect("bounded chart renders");
    assert!(output.contains("Showing 20 of 25 categories and 8 of 10 series"));
    assert!(output.contains("Category 19"));
    assert!(!output.contains("Category 20"));
    assert!(output.contains("Series 7"));
    assert!(!output.contains("Series 8"));
}

#[test]
fn non_finite_and_negative_pie_values_are_reported() {
    let mut chart = sample(ChartType::Line);
    chart.series[0].values[0] = f64::INFINITY;
    assert!(svg(&chart, 400.0, 184.0)
        .unwrap_err()
        .contains("non-finite"));

    let mut chart = sample(ChartType::Pie);
    chart.series[0].values[0] = -1.0;
    assert!(svg(&chart, 400.0, 184.0)
        .unwrap_err()
        .contains("non-negative"));
}

#[test]
fn image_cache_reuses_typed_chart_and_size_and_stays_bounded() {
    IMAGE_CACHE.with(|cache| cache.borrow_mut().clear());
    let mut chart = Arc::new(sample(ChartType::Line));
    image(&chart, 400.0, 184.0).expect("first chart projection");
    image(&chart, 400.0, 184.0).expect("cached chart projection");
    assert_eq!(IMAGE_CACHE.with(|cache| cache.borrow().len()), 1);

    image(&chart, 399.0, 184.0).expect("different size projection");
    Arc::make_mut(&mut chart).series[0].values[0] = 3.5;
    image(&chart, 400.0, 184.0).expect("changed chart projection");
    assert_eq!(IMAGE_CACHE.with(|cache| cache.borrow().len()), 3);

    for index in 0..=IMAGE_CACHE_SIZE {
        Arc::make_mut(&mut chart).title = Some(format!("Chart {index}"));
        image(&chart, 400.0, 184.0).expect("bounded cache entry");
    }
    assert_eq!(
        IMAGE_CACHE.with(|cache| cache.borrow().len()),
        IMAGE_CACHE_SIZE
    );
}

#[test]
fn parsed_chart_cache_hits_and_invalidates_on_new_json_source() {
    CHART_CACHE.with(|cache| cache.borrow_mut().clear());
    IMAGE_CACHE.with(|cache| cache.borrow_mut().clear());

    let mut block = RichBlock::new(17, loom_writer_core::CHART_BLOCK_KIND, "\u{fffc}");
    block.chart = Some(Arc::new(
        sample(ChartType::Column).to_json().expect("chart JSON"),
    ));
    let first = model(&block).expect("parse chart model");
    let hit = model(&block).expect("reuse parsed chart model");
    assert!(Arc::ptr_eq(&first, &hit));
    image(&first, 400.0, 184.0).expect("render initial chart image");

    let mut changed_chart = sample(ChartType::Column);
    changed_chart.series[0].values[0] = 33.0;
    block.chart = Some(Arc::new(
        changed_chart.to_json().expect("updated chart JSON"),
    ));
    let changed = model(&block).expect("parse updated chart model");
    assert!(!Arc::ptr_eq(&first, &changed));
    image(&changed, 400.0, 184.0).expect("render updated chart image");
    assert_eq!(CHART_CACHE.with(|cache| cache.borrow().len()), 1);
    assert_eq!(IMAGE_CACHE.with(|cache| cache.borrow().len()), 2);
}

#[test]
fn oversized_chart_json_reuses_bounded_image_without_retaining_parsed_chart() {
    CHART_CACHE.with(|cache| cache.borrow_mut().clear());
    IMAGE_CACHE.with(|cache| cache.borrow_mut().clear());

    let chart_json = sample(ChartType::Column).to_json().expect("chart JSON");
    let padded = format!(
        "{},\"bounded_test_padding\":\"{}\"}}",
        chart_json.trim_end_matches('}'),
        "x".repeat(MAX_CACHED_CHART_JSON_BYTES + 1)
    );
    let mut block = RichBlock::new(22, loom_writer_core::CHART_BLOCK_KIND, "\u{fffc}");
    block.chart = Some(Arc::new(padded));
    let source = Arc::clone(block.chart.as_ref().expect("chart source"));

    image_for_block(&block, 400.0, 184.0).expect("first large chart image");
    assert_eq!(CHART_CACHE.with(|cache| cache.borrow().len()), 0);
    assert_eq!(IMAGE_CACHE.with(|cache| cache.borrow().len()), 1);
    IMAGE_CACHE.with(|cache| {
        let cache = cache.borrow();
        assert!(cache[0].chart.upgrade().is_none());
        assert!(cache[0]
            .source
            .as_ref()
            .and_then(Weak::upgrade)
            .is_some_and(|cached| Arc::ptr_eq(&cached, &source)));
    });

    image_for_block(&block, 400.0, 184.0).expect("reuse large chart image");
    assert_eq!(IMAGE_CACHE.with(|cache| cache.borrow().len()), 1);
}
