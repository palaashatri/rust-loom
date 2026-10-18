//! Save on a deck opened from a PowerPoint file must not overwrite that file
//! with a Loom deck.

use super::keyboard_flow_tests::launched_with;
use super::*;

#[test]
fn save_after_opening_a_pptx_asks_for_a_loom_file_and_leaves_the_pptx_alone() {
    let dir = std::env::temp_dir().join(format!("loom-present-pptxsave-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let pptx = dir.join("talk.pptx");
    std::fs::write(&pptx, b"original powerpoint bytes").unwrap();
    let target = dir.join("copy.loomdeck");
    let s = launched_with(Rc::new(loom_desktop::ScriptedFileDialogs::new(
        [],
        [Some(target.clone())],
    )));
    *s.state.save_path.borrow_mut() = Some(pptx.clone());
    s.app.invoke_add_slide();

    s.app.invoke_save_deck();

    assert_eq!(
        std::fs::read(&pptx).unwrap(),
        b"original powerpoint bytes",
        "the PowerPoint file must not be overwritten"
    );
    assert!(target.exists(), "the Loom copy was written");
    assert_eq!(
        s.state.save_path.borrow().as_deref(),
        Some(target.as_path()),
        "later saves go to the Loom file"
    );
}

#[test]
fn the_suggested_name_for_a_powerpoint_file_is_a_loom_name() {
    let s = launched_with(Rc::new(loom_desktop::ScriptedFileDialogs::default()));
    *s.state.save_path.borrow_mut() =
        Some(std::path::PathBuf::from("C:/decks/Quarterly Talk.pptx"));
    assert_eq!(
        save_request(&s.state).suggested_name.as_deref(),
        Some("Quarterly Talk.loomdeck")
    );
}
