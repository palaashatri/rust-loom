use super::{
    CommandInvocation, GuiState, InvocationSource, WriterApp, WriterDocument, WriterReviewState,
};
use loom_writer_core::pdf_export_view::PdfRevisionView;
use slint::{ComponentHandle, SharedString};
use std::{path::Path, rc::Rc};

pub(super) fn register(app: &WriterApp, state: &Rc<GuiState>) {
    let global = app.global::<WriterReviewState>();
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        global.on_export_pdf_view(move |selected_view| {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            export_selected_view(&app, &state, selected_view);
        });
    }
    {
        let app_ref = app.as_weak();
        global.on_cancel_pdf_export(move || {
            if let Some(app) = app_ref.upgrade() {
                app.set_status_left("Export cancelled".into());
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_export_pdf(move || {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            let guard = state
                .registry
                .lock()
                .unwrap()
                .invoke(&CommandInvocation::new(
                    "file.export_pdf",
                    InvocationSource::Toolbar,
                ));
            if guard.is_err() {
                app.set_status_right("Add content before exporting PDF".into());
                return;
            }
            app.global::<WriterReviewState>().set_pdf_export_open(true);
        });
    }
}

fn export_selected_view(app: &WriterApp, state: &GuiState, selected_view: i32) {
    app.global::<WriterReviewState>().set_pdf_export_open(false);
    let Some(view) = view_from_selection(selected_view) else {
        app.set_status_left("Choose Final or Review before exporting PDF".into());
        return;
    };

    match state
        .dialogs
        .save_file(&super::writer_export_request(state))
    {
        Ok(Some(path)) => {
            let document = state.current.borrow();
            match super::export_pdf_file(&path, &document, view) {
                Ok(()) => app.set_status_left(SharedString::from(pdf_export_status(
                    &path, &document, view,
                ))),
                Err(error) => {
                    app.set_status_left(SharedString::from(format!("Export failed: {error}")))
                }
            }
        }
        Ok(None) => app.set_status_left("Export cancelled".into()),
        Err(error) => {
            app.set_status_left(SharedString::from(format!("Export dialog failed: {error}")))
        }
    }
}

fn view_from_selection(selection: i32) -> Option<PdfRevisionView> {
    match selection {
        0 => Some(PdfRevisionView::Final),
        1 => Some(PdfRevisionView::Review),
        _ => None,
    }
}

fn pdf_export_status(path: &Path, document: &WriterDocument, view: PdfRevisionView) -> String {
    format!(
        "{} ({} view)",
        super::pdf_export_message(path, document),
        view.name()
    )
}

#[cfg(test)]
mod tests {
    use super::{pdf_export_status, view_from_selection};
    use crate::{export_pdf_file, WriterDocument};
    use loom_writer_core::pdf_export_view::PdfRevisionView;
    use loom_writer_core::{RevisionKind, RevisionPayload, RichBlock};
    use std::{fs, path::PathBuf};

    fn tracked_document() -> WriterDocument {
        let mut document = WriterDocument::new("app-pdf-view", "PDF view");
        document.push(RichBlock::new(1, "paragraph", "Final text"));
        document.tracked_changes.set_recording(true);
        document
            .tracked_changes
            .set_author("Ada")
            .expect("valid author");
        document
            .record_revision(
                RevisionKind::Deletion,
                1,
                10..10,
                Some(RevisionPayload {
                    text: "Draft text".into(),
                    runs: Vec::new(),
                }),
                None,
            )
            .expect("record pending deletion");
        document
    }

    fn output_path(view: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "loom-writer-pdf-view-{}-{view}.pdf",
            std::process::id()
        ))
    }

    #[test]
    fn app_export_writes_the_selected_view_and_names_it_in_status() {
        let document = tracked_document();
        let final_path = output_path("final");
        let review_path = output_path("review");
        let _ = fs::remove_file(&final_path);
        let _ = fs::remove_file(&review_path);

        assert_eq!(view_from_selection(0), Some(PdfRevisionView::Final));
        assert_eq!(view_from_selection(1), Some(PdfRevisionView::Review));
        assert_eq!(view_from_selection(2), None);

        export_pdf_file(&final_path, &document, PdfRevisionView::Final)
            .expect("write final-view PDF");
        export_pdf_file(&review_path, &document, PdfRevisionView::Review)
            .expect("write review-view PDF");
        assert!(fs::metadata(&final_path).expect("final PDF exists").len() > 0);
        assert!(fs::metadata(&review_path).expect("review PDF exists").len() > 0);
        assert_eq!(
            pdf_export_status(&final_path, &document, PdfRevisionView::Final),
            format!("Exported {} (Final view)", final_path.display())
        );
        assert_eq!(
            pdf_export_status(&review_path, &document, PdfRevisionView::Review),
            format!("Exported {} (Review view)", review_path.display())
        );

        let _ = fs::remove_file(final_path);
        let _ = fs::remove_file(review_path);
    }
}
