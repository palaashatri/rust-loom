//! Bounded WordprocessingML import for Writer headers and footers.

use std::collections::BTreeMap;

use loom_package::zip::PackageArchive;
use quick_xml::{
    events::{BytesStart, Event},
    Reader,
};

use super::super::inline_style::{attribute_value, property_tag_with_start};
use super::{FOOTER_REL, HEADER_REL, MAX_PART_XML_BYTES};
use crate::{HeaderFooterConfig, PageNumberFormat, MAX_HEADER_FOOTER_TEXT_BYTES};

pub(super) fn import_config(
    document_xml: &str,
    archive: &PackageArchive,
) -> Result<HeaderFooterConfig, String> {
    let section = section_xml(document_xml)?;
    let has_title_page = parse_on_off_property(&section, "w:titlePg")?;
    if let Some(bytes) = archive.get("word/settings.xml") {
        if bytes.len() > MAX_PART_XML_BYTES {
            return Err("word/settings.xml exceeds the 32768-byte import limit".into());
        }
        let settings = std::str::from_utf8(bytes)
            .map_err(|_| "word/settings.xml is not valid UTF-8".to_string())?;
        if parse_on_off_property(settings, "w:evenAndOddHeaders")? {
            return Err("even-page headers and footers are not supported".into());
        }
    }

    let references = parse_references(&section)?;
    let relationships = parse_relationships(archive)?;
    let header = read_reference(
        &references.header_default,
        HEADER_REL,
        &relationships,
        archive,
    )?;
    let footer = read_reference(
        &references.footer_default,
        FOOTER_REL,
        &relationships,
        archive,
    )?;
    let first_header = read_reference(
        &references.header_first,
        HEADER_REL,
        &relationships,
        archive,
    )?;
    let first_footer = read_reference(
        &references.footer_first,
        FOOTER_REL,
        &relationships,
        archive,
    )?;
    if first_header
        .as_ref()
        .is_some_and(|part| !part.text.is_empty())
        || first_footer
            .as_ref()
            .is_some_and(|part| !part.text.is_empty())
    {
        return Err(
                "custom first-page header/footer content cannot be represented; only first-page suppression is supported"
                    .into(),
            );
    }

    let header_text = header
        .as_ref()
        .map_or_else(String::new, |part| part.text.clone());
    let footer_text = footer
        .as_ref()
        .map_or_else(String::new, |part| part.text.clone());
    let alignment = merge_alignment(
        header
            .as_ref()
            .filter(|part| !part.text.is_empty())
            .map(|part| part.alignment.as_str()),
        footer
            .as_ref()
            .filter(|part| !part.text.is_empty())
            .map(|part| part.alignment.as_str()),
    )?;
    let config = HeaderFooterConfig {
        header_text,
        footer_text,
        alignment: if alignment == "left" {
            String::new()
        } else {
            alignment.into()
        },
        page_number_format: parse_page_number_format(&section)?,
        different_first_page: has_title_page,
    };
    config.validate().map_err(str::to_owned)?;
    Ok(config)
}

fn section_xml(document_xml: &str) -> Result<String, String> {
    let mut reader = Reader::from_str(document_xml);
    let mut found = Vec::new();
    let mut section_start = None;
    loop {
        let before = usize::try_from(reader.buffer_position())
            .map_err(|_| "invalid document XML position".to_string())?;
        match reader.read_event() {
            Ok(Event::Start(tag)) if tag.name().as_ref() == b"w:sectPr" => {
                if section_start.replace(before).is_some() {
                    return Err("nested document section properties are malformed".into());
                }
            }
            Ok(Event::Empty(tag)) if tag.name().as_ref() == b"w:sectPr" => {
                let end = usize::try_from(reader.buffer_position())
                    .map_err(|_| "invalid document XML position".to_string())?;
                found.push(document_xml[before..end].to_string());
            }
            Ok(Event::End(tag)) if tag.name().as_ref() == b"w:sectPr" => {
                let start = section_start
                    .take()
                    .ok_or_else(|| "section properties end without a start".to_string())?;
                let end = usize::try_from(reader.buffer_position())
                    .map_err(|_| "invalid document XML position".to_string())?;
                found.push(document_xml[start..end].to_string());
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => return Err("malformed document.xml while reading section properties".into()),
        }
    }
    if section_start.is_some() {
        return Err("malformed document.xml while reading section properties".into());
    }
    if found.len() > 1 {
        return Err("multiple document sections are not supported for header/footer import".into());
    }
    Ok(found.pop().unwrap_or_default())
}

#[derive(Default)]
struct References {
    header_default: Option<String>,
    header_first: Option<String>,
    footer_default: Option<String>,
    footer_first: Option<String>,
}

fn parse_references(section: &str) -> Result<References, String> {
    let mut reader = Reader::from_str(section);
    let decoder = reader.decoder();
    let mut references = References::default();
    loop {
        match reader.read_event() {
            Ok(Event::Start(tag) | Event::Empty(tag)) => {
                let name = tag.name().as_ref().to_vec();
                if name != b"w:headerReference" && name != b"w:footerReference" {
                    continue;
                }
                let header = name == b"w:headerReference";
                let reference_type =
                    tag_attribute(&tag, b"w:type", decoder)?.unwrap_or_else(|| "default".into());
                let id = tag_attribute(&tag, b"r:id", decoder)?
                    .ok_or_else(|| "header/footer reference is missing r:id".to_string())?;
                let slot = match (header, reference_type.as_str()) {
                    (true, "default") => &mut references.header_default,
                    (true, "first") => &mut references.header_first,
                    (false, "default") => &mut references.footer_default,
                    (false, "first") => &mut references.footer_first,
                    (_, "even") => {
                        return Err("even-page headers and footers are not supported".into())
                    }
                    _ => {
                        return Err(format!(
                            "unsupported header/footer reference type {reference_type}"
                        ))
                    }
                };
                if slot.replace(id).is_some() {
                    return Err("duplicate header/footer section reference".into());
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => return Err("malformed section reference XML".into()),
        }
    }
    Ok(references)
}

#[derive(Debug)]
struct Relationship {
    kind: String,
    target: String,
    external: bool,
}

fn parse_relationships(archive: &PackageArchive) -> Result<BTreeMap<String, Relationship>, String> {
    let Some(bytes) = archive.get("word/_rels/document.xml.rels") else {
        return Ok(BTreeMap::new());
    };
    if bytes.len() > MAX_PART_XML_BYTES {
        return Err("document relationships exceed the 32768-byte import limit".into());
    }
    let source = std::str::from_utf8(bytes)
        .map_err(|_| "document relationships are not valid UTF-8".to_string())?;
    let mut reader = Reader::from_str(source);
    let decoder = reader.decoder();
    let mut relationships = BTreeMap::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(tag) | Event::Empty(tag)) if tag.name().as_ref() == b"Relationship" => {
                let Some(id) = tag_attribute(&tag, b"Id", decoder)? else {
                    continue;
                };
                let Some(kind) = tag_attribute(&tag, b"Type", decoder)? else {
                    continue;
                };
                let target = tag_attribute(&tag, b"Target", decoder)?
                    .ok_or_else(|| format!("relationship {id} is missing Target"))?;
                if target.len() > 1_024 {
                    return Err("relationship target is too long".into());
                }
                let external = tag_attribute(&tag, b"TargetMode", decoder)?
                    .is_some_and(|mode| mode.eq_ignore_ascii_case("external"));
                if relationships
                    .insert(
                        id.clone(),
                        Relationship {
                            kind,
                            target,
                            external,
                        },
                    )
                    .is_some()
                {
                    return Err(format!("duplicate document relationship Id {id}"));
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => return Err("malformed document relationships XML".into()),
        }
    }
    Ok(relationships)
}

#[derive(Debug)]
struct PartContent {
    text: String,
    alignment: String,
}

fn read_reference(
    id: &Option<String>,
    expected_kind: &str,
    relationships: &BTreeMap<String, Relationship>,
    archive: &PackageArchive,
) -> Result<Option<PartContent>, String> {
    let Some(id) = id else { return Ok(None) };
    let relationship = relationships
        .get(id)
        .ok_or_else(|| format!("header/footer relationship {id} is missing"))?;
    if relationship.kind != expected_kind {
        return Err(format!(
            "header/footer relationship {id} has the wrong type"
        ));
    }
    if relationship.external {
        return Err("external header/footer relationships are not supported".into());
    }
    let path = resolve_target(&relationship.target)?;
    let bytes = archive
        .get(&path)
        .ok_or_else(|| format!("header/footer part {path} is missing"))?;
    if bytes.len() > MAX_PART_XML_BYTES {
        return Err("header/footer part exceeds the 32768-byte import limit".into());
    }
    let part_xml = std::str::from_utf8(bytes)
        .map_err(|_| format!("header/footer part {path} is not valid UTF-8"))?;
    parse_part(part_xml).map(Some)
}

fn resolve_target(target: &str) -> Result<String, String> {
    if target.is_empty()
        || target
            .chars()
            .any(|character| matches!(character, '\\' | '%' | ':' | '?' | '#'))
    {
        return Err("invalid header/footer relationship target".into());
    }
    let absolute = target.starts_with('/');
    let mut components: Vec<String> = if absolute {
        Vec::new()
    } else {
        vec!["word".into()]
    };
    for component in target.trim_start_matches('/').split('/') {
        match component {
            "" | "." => {}
            ".." => {
                if components.pop().is_none() {
                    return Err("header/footer relationship escapes the package".into());
                }
            }
            value => components.push(value.to_string()),
        }
    }
    if components.is_empty() {
        return Err("invalid header/footer relationship target".into());
    }
    Ok(components.join("/"))
}

#[derive(Debug)]
struct Field {
    instruction: String,
    result_phase: bool,
    simple: bool,
}

fn parse_part(source: &str) -> Result<PartContent, String> {
    let mut reader = Reader::from_str(source);
    let decoder = reader.decoder();
    let mut output = String::new();
    let mut alignment = None::<String>;
    let mut paragraph_alignment = "left".to_string();
    let mut in_paragraph = false;
    let mut in_text = false;
    let mut in_instruction = false;
    let mut fields: Vec<Field> = Vec::new();
    let mut root_seen = false;
    let mut root_closed = false;
    let mut root_name: Option<Vec<u8>> = None;
    loop {
        match reader.read_event() {
            Ok(Event::Start(tag)) => {
                let name = tag.name().as_ref().to_vec();
                if !root_seen {
                    if name != b"w:hdr" && name != b"w:ftr" {
                        return Err("header/footer part has an unexpected root element".into());
                    }
                    root_seen = true;
                    root_name = Some(name.clone());
                } else if root_closed || name == b"w:hdr" || name == b"w:ftr" {
                    return Err("header/footer part has a nested or trailing root element".into());
                }
                match name.as_slice() {
                    b"w:hdr" | b"w:ftr" | b"w:pPr" | b"w:r" | b"w:rPr" => {}
                    b"w:p" => {
                        if in_paragraph {
                            return Err(
                                "nested paragraphs in header/footer are not supported".into()
                            );
                        }
                        in_paragraph = true;
                        paragraph_alignment = "left".into();
                    }
                    b"w:t" => {
                        if !in_paragraph {
                            return Err("header/footer text is outside a paragraph".into());
                        }
                        in_text = true;
                    }
                    b"w:instrText" => {
                        if !in_paragraph {
                            return Err("header/footer field is outside a paragraph".into());
                        }
                        in_instruction = true;
                    }
                    b"w:jc" => set_alignment(
                        &mut paragraph_alignment,
                        tag_attribute(&tag, b"w:val", decoder)?.as_deref(),
                    )?,
                    b"w:fldSimple" => {
                        if !fields.is_empty() {
                            return Err("nested fields in headers/footers are not supported".into());
                        }
                        let instruction =
                            bounded_instruction(tag_attribute(&tag, b"w:instr", decoder)?)?;
                        fields.push(Field {
                            instruction,
                            result_phase: true,
                            simple: true,
                        });
                    }
                    b"w:tbl" | b"w:drawing" | b"w:pict" | b"w:hyperlink" | b"w:altChunk" => {
                        return Err("header/footer contains unsupported rich content".into());
                    }
                    b"w:fldChar" => apply_field_char(&mut fields, &mut output, &tag, decoder)?,
                    _ => return Err("header/footer contains unsupported WordprocessingML".into()),
                }
            }
            Ok(Event::Empty(tag)) => {
                let name = tag.name().as_ref().to_vec();
                if !root_seen {
                    if name != b"w:hdr" && name != b"w:ftr" {
                        return Err("header/footer part has an unexpected root element".into());
                    }
                    root_seen = true;
                    root_closed = true;
                    root_name = Some(name.clone());
                    continue;
                }
                if root_closed || name == b"w:hdr" || name == b"w:ftr" {
                    return Err("header/footer part has a nested or trailing root element".into());
                }
                match name.as_slice() {
                    b"w:tab" => push_text(&mut output, "\t", &fields)?,
                    b"w:br" => {
                        if tag_attribute(&tag, b"w:type", decoder)?.as_deref() == Some("page") {
                            return Err("page breaks in headers/footers are not supported".into());
                        }
                        push_text(&mut output, "\n", &fields)?;
                    }
                    b"w:jc" => set_alignment(
                        &mut paragraph_alignment,
                        tag_attribute(&tag, b"w:val", decoder)?.as_deref(),
                    )?,
                    b"w:fldChar" => apply_field_char(&mut fields, &mut output, &tag, decoder)?,
                    b"w:fldSimple" => {
                        let instruction =
                            bounded_instruction(tag_attribute(&tag, b"w:instr", decoder)?)?;
                        push_field(&mut output, &instruction, &fields)?;
                    }
                    b"w:p" | b"w:pPr" | b"w:r" | b"w:rPr" | b"w:t" | b"w:proofErr" => {}
                    _ => return Err("header/footer contains unsupported WordprocessingML".into()),
                }
            }
            Ok(Event::Text(event)) => {
                let decoded = event
                    .decode()
                    .map_err(|_| "invalid header/footer text encoding".to_string())?;
                let text = quick_xml::escape::unescape(decoded.as_ref())
                    .map_err(|_| "invalid XML entity in header/footer text".to_string())?;
                if in_instruction {
                    let field = fields
                        .last_mut()
                        .ok_or_else(|| "field instruction without a field".to_string())?;
                    if field.instruction.len() + text.len() > 256 {
                        return Err("header/footer field instruction is too long".into());
                    }
                    field.instruction.push_str(&text);
                } else if in_text {
                    push_text(&mut output, &text, &fields)?;
                } else if !text.trim().is_empty() {
                    return Err("unexpected text outside header/footer content".into());
                }
            }
            Ok(Event::End(tag)) => match tag.name().as_ref() {
                b"w:t" => in_text = false,
                b"w:instrText" => in_instruction = false,
                b"w:fldSimple" => {
                    let field = fields
                        .pop()
                        .ok_or_else(|| "unbalanced simple field".to_string())?;
                    if !field.simple {
                        return Err("unbalanced simple field".into());
                    }
                    push_field(&mut output, &field.instruction, &fields)?;
                }
                b"w:fldChar" => {}
                b"w:p" => {
                    if !in_paragraph {
                        return Err("paragraph end without start".into());
                    }
                    if alignment
                        .as_ref()
                        .is_some_and(|value| value != &paragraph_alignment)
                    {
                        return Err("header/footer paragraphs use different alignments".into());
                    }
                    alignment.get_or_insert_with(|| paragraph_alignment.clone());
                    output.push('\n');
                    in_paragraph = false;
                }
                b"w:hdr" | b"w:ftr" => {
                    if root_closed || root_name.as_deref() != Some(tag.name().as_ref()) {
                        return Err("header/footer root closes more than once".into());
                    }
                    if in_paragraph {
                        return Err("header/footer root closes inside a paragraph".into());
                    }
                    root_closed = true;
                }
                b"w:pPr" | b"w:r" | b"w:rPr" => {}
                _ => return Err("header/footer contains unsupported WordprocessingML".into()),
            },
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => return Err("malformed header/footer XML".into()),
        }
        if output.len() > MAX_HEADER_FOOTER_TEXT_BYTES + 1 {
            return Err("header/footer text exceeds the 2048-byte limit".into());
        }
    }
    if !root_seen || !root_closed {
        return Err("header/footer part is missing a complete root element".into());
    }
    if !fields.is_empty() {
        return Err("unclosed field in header/footer".into());
    }
    if output.ends_with('\n') {
        output.pop();
    }
    if output.len() > MAX_HEADER_FOOTER_TEXT_BYTES {
        return Err("header/footer text exceeds the 2048-byte limit".into());
    }
    Ok(PartContent {
        text: output,
        alignment: alignment.unwrap_or_else(|| "left".into()),
    })
}

fn set_alignment(current: &mut String, value: Option<&str>) -> Result<(), String> {
    let value = value.unwrap_or("left");
    if !matches!(value, "left" | "center" | "right") {
        return Err(format!("unsupported header/footer alignment {value}"));
    }
    *current = value.into();
    Ok(())
}

fn apply_field_char(
    fields: &mut Vec<Field>,
    out: &mut String,
    tag: &BytesStart<'_>,
    decoder: quick_xml::encoding::Decoder,
) -> Result<(), String> {
    let kind = tag_attribute(tag, b"w:fldCharType", decoder)?
        .ok_or_else(|| "field character is missing its type".to_string())?;
    match kind.as_str() {
        "begin" => {
            if !fields.is_empty() {
                return Err("nested fields in headers/footers are not supported".into());
            }
            fields.push(Field {
                instruction: String::new(),
                result_phase: false,
                simple: false,
            });
        }
        "separate" => {
            let field = fields
                .last_mut()
                .ok_or_else(|| "field separator without field begin".to_string())?;
            if field.simple || field.result_phase {
                return Err("invalid field separator in header/footer".into());
            }
            field.result_phase = true;
        }
        "end" => {
            let field = fields
                .pop()
                .ok_or_else(|| "field end without field begin".to_string())?;
            if field.simple || !field.result_phase {
                return Err("invalid complex field end in header/footer".into());
            }
            push_field(out, &field.instruction, fields)?;
        }
        _ => return Err(format!("unsupported header/footer field marker {kind}")),
    }
    Ok(())
}

fn bounded_instruction(instruction: Option<String>) -> Result<String, String> {
    let instruction = instruction.unwrap_or_default();
    if instruction.len() > 256 {
        return Err("header/footer field instruction is too long".into());
    }
    Ok(instruction)
}

fn push_text(out: &mut String, text: &str, fields: &[Field]) -> Result<(), String> {
    if fields.last().is_some_and(|field| field.result_phase) {
        return Ok(());
    }
    if out.len().saturating_add(text.len()) > MAX_HEADER_FOOTER_TEXT_BYTES + 1 {
        return Err("header/footer text exceeds the 2048-byte limit".into());
    }
    out.push_str(text);
    Ok(())
}

fn push_field(out: &mut String, instruction: &str, fields: &[Field]) -> Result<(), String> {
    if fields.last().is_some_and(|field| field.result_phase) {
        return Ok(());
    }
    match instruction
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase()
        .as_str()
    {
        "PAGE" => out.push_str("{page}"),
        "NUMPAGES" => out.push_str("{total}"),
        other => return Err(format!("unsupported header/footer field {other}")),
    }
    Ok(())
}

fn tag_attribute(
    tag: &BytesStart<'_>,
    name: &[u8],
    decoder: quick_xml::encoding::Decoder,
) -> Result<Option<String>, String> {
    for attribute in tag.attributes() {
        let attribute = attribute.map_err(|_| "malformed header/footer attribute".to_string())?;
        if attribute.key.as_ref() == name {
            return attribute
                .decoded_and_normalized_value(quick_xml::XmlVersion::Implicit1_0, decoder)
                .map(|value| Some(value.into_owned()))
                .map_err(|_| "invalid header/footer attribute encoding".to_string());
        }
    }
    Ok(None)
}

fn merge_alignment<'a>(
    header: Option<&'a str>,
    footer: Option<&'a str>,
) -> Result<&'a str, String> {
    match (header, footer) {
        (Some(h), Some(f)) if h != f => Err("header and footer use different alignments".into()),
        (Some(value), _) | (_, Some(value)) => Ok(value),
        (None, None) => Ok("left"),
    }
}

fn parse_page_number_format(section: &str) -> Result<PageNumberFormat, String> {
    let Some((_, tag)) = property_tag_with_start(section, "w:pgNumType") else {
        return Ok(PageNumberFormat::Arabic);
    };
    if attribute_value(tag, "w:start").is_some_and(|value| value != "1") {
        return Err("page-number starts other than 1 are not supported".into());
    }
    match attribute_value(tag, "w:fmt")
        .as_deref()
        .unwrap_or("decimal")
    {
        "decimal" | "decimalZero" => Ok(PageNumberFormat::Arabic),
        "upperRoman" => Ok(PageNumberFormat::RomanUpper),
        "lowerRoman" => Ok(PageNumberFormat::RomanLower),
        "upperLetter" => Ok(PageNumberFormat::Alphabetical),
        _ => Err("unsupported DOCX page-number format".into()),
    }
}

fn parse_on_off_property(source: &str, name: &str) -> Result<bool, String> {
    let Some((_, tag)) = property_tag_with_start(source, name) else {
        return Ok(false);
    };
    match attribute_value(tag, "w:val").as_deref() {
        None | Some("true" | "1" | "on") => Ok(true),
        Some("false" | "0" | "off") => Ok(false),
        Some(_) => Err(format!("unsupported value for {name}")),
    }
}
