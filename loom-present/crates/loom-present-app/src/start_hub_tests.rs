use super::*;
use crate::{start_hub, StartHubState};
use std::path::Path;
use std::time::{Duration, SystemTime};

#[test]
fn start_hub_opens_and_formats_badges() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    assert!(!app.global::<StartHubState>().get_open());
    start_hub::open(&app);
    assert!(app.global::<StartHubState>().get_open());
    assert_eq!(
        start_hub::format_badge(Path::new("deck.loomdeck")),
        ".loomdeck"
    );
    assert_eq!(start_hub::format_badge(Path::new("deck.pptx")), ".pptx");
}

#[test]
fn start_hub_relative_time_label_formats_recent_times() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000);
    assert_eq!(start_hub::relative_time_label(now, now), "Just now");
    let mins_ago = now - Duration::from_secs(180);
    assert_eq!(start_hub::relative_time_label(mins_ago, now), "3m ago");
}

#[test]
fn recent_warned_pptx_open_keeps_document_until_import_confirmation() {
    let dir = std::env::temp_dir().join(format!(
        "loom-present-recent-pptx-warn-{}",
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&dir);
    let pptx_path = dir.join("warned.pptx");
    // Export a PPTX and inject a SmartArt reference (<dgm:relIds>) so prepare() produces a warning.
    let src_session = crate::sample_session();
    let base_bytes = loom_present_core::export_pptx(&src_session).expect("export");
    let mut pkg = loom_package::PackageArchive::from_bytes(&base_bytes).expect("open pkg");
    pkg.add("ppt/diagrams/layout1.xml", b"<dgm:relIds/>".to_vec())
        .expect("add smartart diagram part");
    std::fs::write(&pptx_path, pkg.to_bytes().expect("bytes")).expect("write pptx");

    let rec_dir = dir.join("recovery");
    std::fs::create_dir_all(&rec_dir).expect("rec dir");
    let mut rec = loom_production::snapshot::SnapshotRecovery::open_at(&rec_dir).expect("rec");
    rec.record("seed", b"prior-draft".to_vec()).expect("seed");
    drop(rec);
    let rec = loom_production::snapshot::SnapshotRecovery::open_at(&rec_dir).expect("reopen rec");
    let prev_recovery = crate::PRESENT_RECOVERY.with(|slot| slot.borrow_mut().replace(rec));

    let (app, state) = crate::element_style_tests::fixture(vec![]);
    crate::table_actions::wire(&app, &state);
    crate::modal_wiring::wire(&app, &state);
    let initial_title = state.session.borrow().document.title.clone();
    start_hub::open(&app);
    assert!(app.global::<StartHubState>().get_open());

    app.global::<StartHubState>()
        .invoke_open_deck(pptx_path.to_string_lossy().as_ref().into());
    assert!(app.get_pptx_import_warning_open());
    assert!(!app.global::<StartHubState>().get_open());
    assert_eq!(state.session.borrow().document.title, initial_title);
    assert!(crate::table_actions::PENDING_PPTX_IMPORT.with(|s| s.borrow().is_some()));
    let preserved = loom_production::snapshot::SnapshotRecovery::open_at(&rec_dir)
        .expect("check rec")
        .restored_payload()
        .map(ToOwned::to_owned);
    assert_eq!(preserved.as_deref(), Some(b"prior-draft".as_slice()));

    app.invoke_pptx_import_cancel();
    assert!(!app.get_pptx_import_warning_open());
    assert_eq!(state.session.borrow().document.title, initial_title);
    assert!(crate::table_actions::PENDING_PPTX_IMPORT.with(|s| s.borrow().is_none()));
    crate::recovery_deferred::invalidate();
    crate::PRESENT_RECOVERY.with(|slot| *slot.borrow_mut() = prev_recovery);
    let _ = std::fs::remove_dir_all(&dir);
}
