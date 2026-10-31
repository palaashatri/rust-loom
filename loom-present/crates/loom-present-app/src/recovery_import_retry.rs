//! Recovery retry guards while a lossy PowerPoint import is pending.

use super::*;

#[test]
fn pending_import_warning_defers_automatic_and_explicit_recovery_retries() {
    let mut harness = Harness::install();
    let baseline = sample_session();
    let baseline_bytes = recovery_draft::payload(&baseline, None).expect("baseline draft");
    let mut recovery = RecoveryScratch::with_service(&baseline_bytes);
    let fault = AppendFault::once(recovery.directory());
    let (app, state) = app_with_session(baseline);
    let latest = edit_by_adding_slide(&app, &state);

    app.invoke_recovery_flush();
    let started = harness.receive_started();
    started.release();
    run_production_completion(harness.receive_delivery());
    assert_eq!(fault.hits(), 1);
    assert!(recovery_deferred::retry_is_armed_for_test());
    assert_eq!(harness.launch_requests(), 1);

    // Opening a lossy PowerPoint warning freezes the retry timers.
    table_actions::PENDING_PPTX_IMPORT.with(|slot| {
        *slot.borrow_mut() = Some(table_actions::PendingPptxImport {
            path: recovery.directory().join("lossy.pptx"),
            session: sample_session(),
            warnings: vec!["SmartArt diagrams will be skipped".into()],
            kept: None,
            preserved_path: None,
            cancel_must_checkpoint_visible_session: false,
        });
    });
    app.set_pptx_import_warning_open(true);
    recovery_deferred::hold_for_import_warning();
    assert!(!recovery_deferred::retry_is_armed_for_test());

    // The warning's own status projection must not re-arm a write that the
    // pending import cannot safely accept.
    set_status(&app, "Review the import warnings before opening this deck");
    assert!(
        !recovery_deferred::retry_is_armed_for_test(),
        "an import warning status update must not re-arm recovery retries"
    );

    // An explicit Retry is deferred too, including its worker.
    app.invoke_recovery_retry();
    keyboard_flow_tests::render(&app);
    assert_eq!(
        harness.launch_requests(),
        1,
        "explicit Retry must not start a recovery write while the import is pending"
    );
    assert!(!recovery_deferred::retry_is_armed_for_test());
    assert!(
        !recovery_deferred::auto_retry_used_for_test(),
        "a deferred Retry must not consume the retry budget"
    );

    // Cancelling the import releases the deck: the latest visible edits are
    // recovered with the original append fault already consumed.
    app.invoke_pptx_import_cancel();
    assert!(table_actions::PENDING_PPTX_IMPORT.with(|slot| slot.borrow().is_none()));
    assert!(!app.get_pptx_import_warning_open());
    app.invoke_recovery_flush();
    let resumed = harness.receive_started();
    resumed.release();
    run_production_completion(harness.receive_delivery());
    assert!(!recovery_deferred::is_stale());
    assert_same_content(&persisted_session(&mut recovery), &latest);
    assert!(
        accessible_label_containing(&app, "no space left on device").is_none(),
        "the recovered deck clears the stale append error"
    );
}
