//! Shared helpers for WordprocessingML character properties.

/// Returns one normalized six-digit RGB value for an opaque Writer color.
///
/// Writer stores colors as `#RRGGBB` or `#RRGGBBAA`; Word stores opaque RGB
/// without the hash or alpha byte. Transparent colors cannot round-trip.
pub(super) fn docx_rgb(color: &str) -> Option<String> {
    let hex = color.strip_prefix('#')?;
    if !hex.is_ascii() {
        return None;
    }
    let rgb = match hex.len() {
        6 => hex,
        8 if hex[6..].eq_ignore_ascii_case("FF") => &hex[..6],
        _ => return None,
    };
    if !rgb.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some(rgb.to_ascii_uppercase())
}

/// Converts Word's six-digit RGB property to Writer's opaque RGBA form.
pub(super) fn writer_color(value: &str) -> Option<String> {
    if value.len() != 6 || !value.is_ascii() || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return None;
    }
    Some(format!("#{}FF", value.to_ascii_uppercase()))
}

/// Finds the opening tag for an exact Word property name.
///
/// The returned slice ends at that tag's `>`, so attributes cannot leak in
/// from a later sibling. A boundary check keeps `w:szCs` distinct from `w:sz`.
pub(super) fn property_tag<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("<{name}");
    let mut offset = 0;
    while let Some(found) = xml[offset..].find(&needle) {
        let start = offset + found;
        let after_name = start + needle.len();
        if matches!(
            xml.as_bytes().get(after_name).copied(),
            Some(b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>')
        ) {
            let close = xml[after_name..].find('>')? + after_name;
            return Some(&xml[start..=close]);
        }
        offset = after_name;
    }
    None
}
