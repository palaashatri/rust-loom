from pathlib import Path

path = Path(__file__).resolve().parents[1] / "loom-writer/crates/loom-writer-app/src/main.rs"
text = path.read_text(encoding="utf-8")
old = '''        history.record(second, third.clone(), HistoryKind::DocumentAction, 200);

        assert_eq!(history.undo_len(), 2);
        assert_eq!(history.undo(), Some(second));
        assert_eq!(history.undo(), Some(first));
        assert_eq!(history.redo(), Some(second));
'''
new = '''        history.record(
            second.clone(),
            third.clone(),
            HistoryKind::DocumentAction,
            200,
        );

        assert_eq!(history.undo_len(), 2);
        assert_eq!(history.undo(), Some(second.clone()));
        assert_eq!(history.undo(), Some(first));
        assert_eq!(history.redo(), Some(second));
'''
if text.count(old) != 1:
    raise RuntimeError(f"expected one Writer history test block, found {text.count(old)}")
path.write_text(text.replace(old, new, 1), encoding="utf-8", newline="\n")
