//! Screenshot and smoke rendering through the production Slint component.

use super::*;

pub(super) fn render_headless(args: &Args, out: &str) -> Result<(), String> {
    set_platform();
    let app = SheetsApp::new().map_err(|e| e.to_string())?;
    app.set_local_menu_visible(!cfg!(target_os = "macos"));
    configure_direction(&app, args.rtl);
    apply_theme(&app, &args.theme);
    app.set_template_text_scale(args.text_scale);
    let (w, h) = args.size;
    app.window().set_size(PhysicalSize::new(w, h));
    apply_layout_breakpoints(&app, w);
    if args.inspector {
        app.set_inspector_preference(true);
        app.set_show_inspector(true);
    }
    apply_headless_viewport_size(&app, w, h);
    let mut sheet = match &args.open {
        Some(p) => load_sheet(Path::new(p))?,
        None if args.example || args.smoke || args.chart || args.objects => starter_workbook(),
        None => blank_sheet(),
    };
    if args.objects {
        object_actions::seed_demo_objects(&mut sheet);
        app.set_selected_object(0);
    }
    if let Some(zoom) = args.zoom {
        app.set_zoom_factor(zoom);
    }
    project_sheet(&app, &sheet);
    if args.palette {
        app.set_palette_query(SharedString::from("ex"));
        rebuild_palette(&app, "ex");
        // Keep the preview selection within the one matching export command
        // so its only row stays visible in the palette.
        app.set_palette_selected(0);
        app.set_palette_open(true);
    }
    if args.template_chooser {
        app.set_template_chooser_open(true);
    }
    if args.chart {
        let planned = if sheet.name == "Example Budget" {
            plan_chart_in_range(&sheet, 0, 1, 1, 3)
        } else {
            plan_chart(&sheet, 0, 1)
        };
        if let Ok(chart) = planned {
            sheet.chart = Some(chart);
            app.set_chart_visible(true);
            sync_chart_to_app(&app, &sheet);
        }
    }
    let img = snapshot_component(&app, w as f32, h as f32, 1.0).map_err(|e| e.to_string())?;
    loom_test_support::png::save_png(Path::new(out), &img).map_err(|e| e.to_string())?;
    Ok(())
}
