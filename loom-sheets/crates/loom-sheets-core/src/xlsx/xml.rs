//! Read the small XML pieces used by the XLSX importer and exporter.

use std::collections::BTreeMap;

use quick_xml::events::Event;
use quick_xml::name::ResolveResult;
use quick_xml::NsReader;

pub(super) struct XmlNode {
    pub(super) namespace: String,
    pub(super) name: String,
    pub(super) attributes: BTreeMap<String, String>,
}

impl XmlNode {
    pub(super) fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes.get(name).map(String::as_str)
    }
}

pub(super) fn parse_xml_nodes(path: &str, xml: &str) -> Result<Vec<XmlNode>, String> {
    let mut reader = NsReader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut nodes = Vec::new();
    loop {
        let (resolved_namespace, event) = reader
            .read_resolved_event()
            .map_err(|error| format!("parse {path}: {error}"))?;
        let start = match event {
            Event::Start(element) | Event::Empty(element) => element,
            Event::Eof => break,
            _ => continue,
        };
        let namespace = match resolved_namespace {
            ResolveResult::Bound(namespace) => {
                String::from_utf8_lossy(namespace.as_ref()).into_owned()
            }
            ResolveResult::Unbound => String::new(),
            ResolveResult::Unknown(prefix) => {
                return Err(format!(
                    "parse {path}: XML element uses undeclared namespace prefix {}",
                    String::from_utf8_lossy(&prefix)
                ));
            }
        };
        let name = String::from_utf8_lossy(start.local_name().as_ref()).into_owned();
        let mut attributes = BTreeMap::new();
        for attribute in start.attributes().with_checks(false) {
            let attribute = attribute.map_err(|error| format!("parse {path}: {error}"))?;
            let key = String::from_utf8_lossy(attribute.key.as_ref()).into_owned();
            let value = attribute
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .map_err(|error| format!("parse {path}: {error}"))?
                .into_owned();
            attributes.insert(key, value);
        }
        nodes.push(XmlNode {
            namespace,
            name,
            attributes,
        });
    }
    Ok(nodes)
}

#[derive(Debug, Clone, Copy)]
pub(super) struct XmlElement<'a> {
    pub(super) attrs: &'a str,
    pub(super) body: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct XmlSpan<'a> {
    pub(super) open_start: usize,
    pub(super) open_end: usize,
    pub(super) attrs: &'a str,
}

pub(super) fn elements<'a>(xml: &'a str, wanted: &str) -> std::vec::IntoIter<XmlElement<'a>> {
    let mut found = Vec::new();
    let mut cursor = 0;
    while let Some(relative) = xml[cursor..].find('<') {
        let start = cursor + relative;
        let Some(end_relative) = xml[start..].find('>') else {
            break;
        };
        let end = start + end_relative;
        let raw = &xml[start + 1..end];
        if raw.starts_with('/') || raw.starts_with('!') || raw.starts_with('?') {
            cursor = end + 1;
            continue;
        }
        let name_end = raw
            .find(|character: char| character.is_whitespace() || character == '/')
            .unwrap_or(raw.len());
        let raw_name = &raw[..name_end];
        if local_name(raw_name) != wanted {
            cursor = end + 1;
            continue;
        }
        let attrs = raw[name_end..].trim_end_matches('/').trim();
        if raw.trim_end().ends_with('/') {
            found.push(XmlElement { attrs, body: "" });
            cursor = end + 1;
            continue;
        }
        let body_start = end + 1;
        if let Some((close_start, close_end)) = matching_close(xml, body_start, wanted) {
            found.push(XmlElement {
                attrs,
                body: &xml[body_start..close_start],
            });
            cursor = close_end;
        } else {
            found.push(XmlElement {
                attrs,
                body: &xml[body_start..],
            });
            cursor = xml.len();
        }
    }
    found.into_iter()
}

pub(super) fn elements_with_offsets<'a>(xml: &'a str, wanted: &str) -> Vec<XmlSpan<'a>> {
    let mut found = Vec::new();
    let mut cursor = 0;
    while let Some(relative) = xml[cursor..].find('<') {
        let start = cursor + relative;
        let Some(end_relative) = xml[start..].find('>') else {
            break;
        };
        let end = start + end_relative + 1;
        let raw = &xml[start + 1..end - 1];
        if raw.starts_with('/') || raw.starts_with('!') || raw.starts_with('?') {
            cursor = end;
            continue;
        }
        let name_end = raw
            .find(|character: char| character.is_whitespace() || character == '/')
            .unwrap_or(raw.len());
        if local_name(&raw[..name_end]) == wanted {
            found.push(XmlSpan {
                open_start: start,
                open_end: end,
                attrs: raw[name_end..].trim_end_matches('/').trim(),
            });
        }
        cursor = end;
    }
    found
}

fn matching_close(xml: &str, body_start: usize, wanted: &str) -> Option<(usize, usize)> {
    let mut cursor = body_start;
    let mut depth = 1usize;
    while let Some(relative) = xml[cursor..].find('<') {
        let start = cursor + relative;
        let end = start + xml[start..].find('>')?;
        let raw = &xml[start + 1..end];
        if let Some(stripped) = raw.strip_prefix('/') {
            let name = stripped.trim();
            if local_name(name) == wanted {
                depth -= 1;
                if depth == 0 {
                    return Some((start, end + 1));
                }
            }
        } else if !raw.starts_with('!') && !raw.starts_with('?') {
            let name_end = raw
                .find(|character: char| character.is_whitespace() || character == '/')
                .unwrap_or(raw.len());
            if local_name(&raw[..name_end]) == wanted && !raw.trim_end().ends_with('/') {
                depth += 1;
            }
        }
        cursor = end + 1;
    }
    None
}

fn local_name(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

pub(super) fn contains_element(xml: &str, wanted: &str) -> bool {
    elements(xml, wanted).next().is_some()
}

pub(super) fn attr(attrs: &str, wanted: &str) -> Option<String> {
    let mut cursor = 0;
    let bytes = attrs.as_bytes();
    while cursor < bytes.len() {
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() {
            break;
        }
        let key_start = cursor;
        while cursor < bytes.len() && !bytes[cursor].is_ascii_whitespace() && bytes[cursor] != b'='
        {
            cursor += 1;
        }
        let key = &attrs[key_start..cursor];
        while cursor < bytes.len() && (bytes[cursor].is_ascii_whitespace() || bytes[cursor] == b'=')
        {
            cursor += 1;
        }
        if cursor >= bytes.len() {
            break;
        }
        let quote = bytes[cursor];
        if quote != b'"' && quote != b'\'' {
            while cursor < bytes.len() && !bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            continue;
        }
        cursor += 1;
        let value_start = cursor;
        while cursor < bytes.len() && bytes[cursor] != quote {
            cursor += 1;
        }
        let value = attrs[value_start..cursor].to_string();
        if key == wanted {
            return Some(value);
        }
        cursor = cursor.saturating_add(1);
    }
    None
}

pub(super) fn xml_escape_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub(super) fn xml_escape_attr(value: &str) -> String {
    xml_escape_text(value)
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub(super) fn xml_unescape(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&gt;", ">")
        .replace("&lt;", "<")
        .replace("&amp;", "&")
}
