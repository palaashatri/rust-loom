from pathlib import Path

path = Path(__file__).resolve().parents[1] / "loom-writer/crates/loom-writer-app/src/main.rs"
text = path.read_text()
old = '''        assert_eq!(history.undo_len(), 2);
        assert_eq!(history.undo(), Some(first.clone()));
        assert_eq!(history.undo(), Some(first));
        assert_eq!(history.redo(), Some(third));
'''
new = '''        assert_eq!(history.undo_len(), 2);
        assert_eq!(history.undo(), Some(second));
        assert_eq!(history.undo(), Some(first));
        assert_eq!(history.redo(), Some(second));
        assert_eq!(history.redo(), Some(third));
'''
if text.count(old) != 1:
    raise RuntimeError(f"expected one Writer document-action test block, found {text.count(old)}")
path.write_text(text.replace(old, new, 1))
