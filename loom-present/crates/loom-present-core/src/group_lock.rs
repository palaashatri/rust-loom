use crate::Slide;
use std::collections::HashSet;

/// Groups the given element IDs under a newly generated `group_id`.
/// Returns `None` if fewer than 2 matching elements are found.
pub fn group_elements(slide: &mut Slide, ids: &[String]) -> Option<String> {
    let id_set: HashSet<&str> = ids.iter().map(String::as_str).collect();
    let count = slide
        .elements
        .iter()
        .filter(|e| id_set.contains(e.id.as_str()))
        .count();
    if count < 2 {
        return None;
    }
    let next_num = slide
        .elements
        .iter()
        .filter_map(|e| e.group_id.as_deref())
        .filter_map(|g| g.strip_prefix("group-")?.parse::<usize>().ok())
        .max()
        .unwrap_or(0)
        + 1;
    let group_id = format!("group-{next_num}");
    for el in &mut slide.elements {
        if id_set.contains(el.id.as_str()) {
            el.group_id = Some(group_id.clone());
        }
    }
    Some(group_id)
}

/// Ungroups all elements belonging to `group_id`. Returns the number of ungrouped elements.
pub fn ungroup_elements(slide: &mut Slide, group_id: &str) -> usize {
    let mut count = 0;
    for el in &mut slide.elements {
        if el.group_id.as_deref() == Some(group_id) {
            el.group_id = None;
            count += 1;
        }
    }
    count
}

/// Sets `locked` on all elements whose IDs are in `ids`. Returns the number updated.
pub fn set_locked(slide: &mut Slide, ids: &[String], locked: bool) -> usize {
    let id_set: HashSet<&str> = ids.iter().map(String::as_str).collect();
    let mut count = 0;
    for el in &mut slide.elements {
        if id_set.contains(el.id.as_str()) {
            el.locked = locked;
            count += 1;
        }
    }
    count
}

/// Toggles `flip_h` (when `horizontal` is true) or `flip_v` (when false) on `ids`,
/// skipping locked elements. Returns the number of elements flipped.
pub fn flip_elements(slide: &mut Slide, ids: &[String], horizontal: bool) -> usize {
    let id_set: HashSet<&str> = ids.iter().map(String::as_str).collect();
    let mut count = 0;
    for el in &mut slide.elements {
        if id_set.contains(el.id.as_str()) && !el.locked {
            if horizontal {
                el.flip_h = !el.flip_h;
            } else {
                el.flip_v = !el.flip_v;
            }
            count += 1;
        }
    }
    count
}

/// Returns true if the element with `id` exists and is locked.
pub fn is_element_locked(slide: &Slide, id: &str) -> bool {
    slide
        .elements
        .iter()
        .find(|e| e.id == id)
        .is_some_and(|e| e.locked)
}

/// Expands a selection of element IDs to include all members of any referenced `group_id`.
pub fn expand_group_selection(slide: &Slide, ids: &[String]) -> Vec<String> {
    let id_set: HashSet<&str> = ids.iter().map(String::as_str).collect();
    let group_ids: HashSet<&str> = slide
        .elements
        .iter()
        .filter(|e| id_set.contains(e.id.as_str()))
        .filter_map(|e| e.group_id.as_deref())
        .collect();
    slide
        .elements
        .iter()
        .filter(|e| {
            id_set.contains(e.id.as_str())
                || e.group_id.as_deref().is_some_and(|g| group_ids.contains(g))
        })
        .map(|e| e.id.clone())
        .collect()
}
