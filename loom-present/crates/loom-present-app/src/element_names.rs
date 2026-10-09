//! Accessible names for slide objects: "Title: Quarterly plan" for an object
//! with text, "Title, empty" for one without, and a "Selected " prefix on the
//! object that is selected.

/// The name a screen reader gives a slide object.
pub(crate) fn label(kind: &str, text: &str, selected: bool) -> String {
    let name = if text.trim().is_empty() {
        format!("{kind}, empty")
    } else {
        format!("{kind}: {text}")
    };
    if selected {
        format!("Selected {name}")
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use super::label;

    #[test]
    fn an_object_without_text_says_so_and_one_with_text_gives_it() {
        assert_eq!(label("Title", "", false), "Title, empty");
        assert_eq!(label("Title", "   ", false), "Title, empty");
        assert_eq!(
            label("Title", "Quarterly plan", false),
            "Title: Quarterly plan"
        );
    }

    #[test]
    fn the_selected_object_is_announced_first() {
        assert_eq!(
            label("Body text", "Hello", true),
            "Selected Body text: Hello"
        );
        assert_eq!(label("Picture", "", true), "Selected Picture, empty");
    }
}
