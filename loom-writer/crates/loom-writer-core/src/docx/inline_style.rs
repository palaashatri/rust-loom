//! Shared helpers for WordprocessingML character properties.

use quick_xml::{events::Event, Reader};

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

/// Returns the first exact attribute value in the event stream.
///
/// Callers may pass one opening tag or a larger XML fragment; in the latter
/// case, the first matching attribute in document order wins. Names are raw
/// QNames, matching the importer's existing prefix contract.
pub(super) fn attribute_value(xml: &str, name: &str) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    let decoder = reader.decoder();
    loop {
        match reader.read_event().ok()? {
            Event::Start(tag) | Event::Empty(tag) => {
                let mut found = None;
                for attribute in tag.attributes() {
                    let attribute = attribute.ok()?;
                    if found.is_none() && attribute.key.as_ref() == name.as_bytes() {
                        found = Some(
                            attribute
                                .decoded_and_normalized_value(
                                    quick_xml::XmlVersion::Implicit1_0,
                                    decoder,
                                )
                                .ok()?
                                .into_owned(),
                        );
                    }
                }
                if found.is_some() {
                    return found;
                }
            }
            Event::Eof => return None,
            _ => {}
        }
    }
}

/// Finds the first exact opening element and returns its original XML slice.
///
/// The event parser skips comments and respects quoted attribute values when
/// finding the tag boundary. Malformed tags fail closed.
pub(super) fn property_tag<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    property_tag_with_start(xml, name).map(|(_, tag)| tag)
}

/// Finds the first exact opening element and returns its byte offset and slice.
pub(super) fn property_tag_with_start<'a>(xml: &'a str, name: &str) -> Option<(usize, &'a str)> {
    let mut reader = Reader::from_str(xml);
    loop {
        let start = usize::try_from(reader.buffer_position()).ok()?;
        match reader.read_event().ok()? {
            Event::Start(tag) | Event::Empty(tag) => {
                let matches = tag.name().as_ref() == name.as_bytes();
                if !tag.attributes().all(|attribute| attribute.is_ok()) {
                    return None;
                }
                if matches {
                    let end = usize::try_from(reader.buffer_position()).ok()?;
                    return Some((start, xml.get(start..end)?));
                }
            }
            Event::Eof => return None,
            _ => {}
        }
    }
}
