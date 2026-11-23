use std::rc::Rc;

use slint::{ComponentHandle, SharedString};

use crate::{PresentApp, PrintPreviewState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrintLayout {
    SingleSlide,
    Handouts2,
    Handouts4,
    Handouts6,
    NotesPage,
}

impl PrintLayout {
    pub(crate) fn from_index(index: i32) -> Self {
        match index {
            1 => Self::Handouts2,
            2 => Self::Handouts4,
            3 => Self::Handouts6,
            4 => Self::NotesPage,
            _ => Self::SingleSlide,
        }
    }

    pub(crate) fn slides_per_page(self) -> usize {
        match self {
            Self::SingleSlide | Self::NotesPage => 1,
            Self::Handouts2 => 2,
            Self::Handouts4 => 4,
            Self::Handouts6 => 6,
        }
    }
}

pub(crate) fn total_pages_for(slide_count: usize, layout: PrintLayout) -> usize {
    let per_page = layout.slides_per_page().max(1);
    slide_count.max(1).div_ceil(per_page)
}

pub(crate) fn open(app: &PresentApp, slide_count: usize, title: &str, notes: &str) {
    let preview = app.global::<PrintPreviewState>();
    let layout = PrintLayout::from_index(preview.get_layout_index());
    let total = total_pages_for(slide_count, layout) as i32;
    preview.set_current_page(1);
    preview.set_total_pages(total);
    preview.set_current_slide_title(SharedString::from(title));
    preview.set_current_slide_notes(SharedString::from(notes));
    preview.set_open(true);
    let toggled = !preview.get_focus_request();
    preview.set_focus_request(toggled);
    app.set_status_left("Opened Print Preview".into());
}

pub(crate) fn wire(
    app: &PresentApp,
    on_export_pdf: Rc<dyn Fn(&PresentApp)>,
    on_refresh: Rc<dyn Fn(&PresentApp)>,
) {
    let preview = app.global::<PrintPreviewState>();

    let weak = app.as_weak();
    preview.on_close_preview(move || {
        if let Some(app) = weak.upgrade() {
            app.global::<PrintPreviewState>().set_open(false);
        }
    });

    let weak = app.as_weak();
    let refresh_prev = Rc::clone(&on_refresh);
    preview.on_prev_page(move || {
        if let Some(app) = weak.upgrade() {
            let p = app.global::<PrintPreviewState>();
            let cur = p.get_current_page();
            if cur > 1 {
                p.set_current_page(cur - 1);
                refresh_prev(&app);
            }
        }
    });

    let weak = app.as_weak();
    let refresh_next = Rc::clone(&on_refresh);
    preview.on_next_page(move || {
        if let Some(app) = weak.upgrade() {
            let p = app.global::<PrintPreviewState>();
            let cur = p.get_current_page();
            if cur < p.get_total_pages() {
                p.set_current_page(cur + 1);
                refresh_next(&app);
            }
        }
    });

    let weak = app.as_weak();
    preview.on_settings_changed(move || {
        if let Some(app) = weak.upgrade() {
            on_refresh(&app);
        }
    });

    let weak = app.as_weak();
    preview.on_export_pdf(move || {
        if let Some(app) = weak.upgrade() {
            app.global::<PrintPreviewState>().set_open(false);
            on_export_pdf(&app);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_pages_calculates_handouts_correctly() {
        assert_eq!(total_pages_for(7, PrintLayout::SingleSlide), 7);
        assert_eq!(total_pages_for(7, PrintLayout::Handouts2), 4);
        assert_eq!(total_pages_for(7, PrintLayout::Handouts4), 2);
        assert_eq!(total_pages_for(7, PrintLayout::Handouts6), 2);
        assert_eq!(total_pages_for(0, PrintLayout::Handouts4), 1);
    }
}
