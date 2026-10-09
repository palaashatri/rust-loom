//! The order of Present's File menu. The shared menu code puts New, Open, Save
//! and Save As first and Exit last, with Present's own commands in between.
//! This keeps each kind of command together and separates the groups: making a
//! deck, saving it, exporting it.

use loom_desktop::{MenuBar, MenuItem};

/// The File commands in display order, one group per line.
const GROUPS: [&[&str]; 3] = [
    &["file.new", "file.new_sample", "file.open"],
    &["file.save", "file.save_as"],
    &["file.export_pdf", "file.export_pptx"],
];

/// Regroups the File menu in place. Commands that are not in `GROUPS` (Exit)
/// follow the last group.
pub(crate) fn group(menu_bar: &mut MenuBar) {
    let Some(file) = menu_bar.menus.iter_mut().find(|menu| menu.title == "File") else {
        return;
    };
    let mut rest: Vec<MenuItem> = std::mem::take(&mut file.items)
        .into_iter()
        .filter(|item| !matches!(item, MenuItem::Separator))
        .collect();
    let mut ordered = Vec::new();
    for ids in GROUPS {
        let group: Vec<MenuItem> = ids.iter().filter_map(|id| take(&mut rest, id)).collect();
        push_group(&mut ordered, group);
    }
    push_group(&mut ordered, rest);
    file.items = ordered;
}

fn take(items: &mut Vec<MenuItem>, id: &str) -> Option<MenuItem> {
    let index = items.iter().position(|item| item.id() == Some(id))?;
    Some(items.remove(index))
}

fn push_group(ordered: &mut Vec<MenuItem>, group: Vec<MenuItem>) {
    if group.is_empty() {
        return;
    }
    if !ordered.is_empty() {
        ordered.push(MenuItem::Separator);
    }
    ordered.extend(group);
}
