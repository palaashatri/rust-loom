use std::collections::HashSet;

use super::*;

fn payload_json(payload: &RevisionPayload) -> String {
    format!(
        "{{\"text\":{},\"runs\":{}}}",
        crate::pkg_json::escape(&payload.text),
        crate::style_json::runs_json(&payload.runs),
    )
}

pub(super) fn block_formats_json(
    block_formats: &std::collections::BTreeMap<u64, super::RevisionBlockFormatChange>,
) -> String {
    let mut out = String::from("[");
    for (index, (revision_id, change)) in block_formats.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"revision_id\":{revision_id},\"before\":{},\"after\":{}}}",
            block_format_json(&change.before),
            block_format_json(&change.after),
        ));
    }
    out.push(']');
    out
}

fn block_format_json(format: &super::RevisionBlockFormat) -> String {
    format!(
        "{{\"kind\":{},\"style\":{}}}",
        crate::pkg_json::escape(&format.kind),
        crate::style_json::paragraph_style_json(&format.style),
    )
}

pub(super) fn parse_block_formats(
    raw: &str,
) -> Result<std::collections::BTreeMap<u64, super::RevisionBlockFormatChange>, WriterError> {
    let entries = crate::ContentParser::new(raw)
        .parse_array_bounded(super::MAX_REVISIONS, super::MAX_REVISION_ENTRY_JSON_BYTES)?;
    let mut block_formats = std::collections::BTreeMap::new();
    for entry in entries {
        let (revision_id, change) = parse_block_format_change(&entry)?;
        if block_formats.insert(revision_id, change).is_some() {
            return Err(WriterError::Json("duplicate revision block format".into()));
        }
    }
    Ok(block_formats)
}

fn parse_block_format_change(
    raw: &str,
) -> Result<(u64, super::RevisionBlockFormatChange), WriterError> {
    let mut revision_id = None;
    let mut before = None;
    let mut after = None;
    let mut seen = std::collections::HashSet::new();
    for (key, value) in crate::ContentParser::new(raw).parse()? {
        if !seen.insert(key.clone()) {
            return Err(WriterError::Json(
                "duplicate revision block format field".into(),
            ));
        }
        match key.as_str() {
            "revision_id" => revision_id = Some(parse_u64(&value, "bad revision block format ID")?),
            "before" => before = Some(parse_block_format(&value)?),
            "after" => after = Some(parse_block_format(&value)?),
            _ => {}
        }
    }
    Ok((
        revision_id
            .ok_or_else(|| WriterError::Json("block format is missing its revision ID".into()))?,
        super::RevisionBlockFormatChange {
            before: before.ok_or_else(|| {
                WriterError::Json("block format is missing its before state".into())
            })?,
            after: after.ok_or_else(|| {
                WriterError::Json("block format is missing its after state".into())
            })?,
        },
    ))
}

fn parse_block_format(value: &crate::JsonValue) -> Result<super::RevisionBlockFormat, WriterError> {
    let crate::JsonValue::Raw(raw) = value else {
        return Err(WriterError::Json(
            "expected revision block format object".into(),
        ));
    };
    let mut kind = None;
    let mut style = None;
    let mut seen = std::collections::HashSet::new();
    for (key, value) in crate::ContentParser::new(raw).parse()? {
        if !seen.insert(key.clone()) {
            return Err(WriterError::Json(
                "duplicate revision block format field".into(),
            ));
        }
        match key.as_str() {
            "kind" => kind = Some(parse_string(&value, "bad revision block kind")?),
            "style" => {
                let crate::JsonValue::Raw(raw) = value else {
                    return Err(WriterError::Json(
                        "expected revision paragraph style object".into(),
                    ));
                };
                style = Some(crate::style_json::parse_paragraph_style(
                    &raw,
                    loom_text::ParagraphStyle::default(),
                )?);
            }
            _ => {}
        }
    }
    Ok(super::RevisionBlockFormat {
        kind: kind.ok_or_else(|| WriterError::Json("block format is missing its kind".into()))?,
        style: style
            .ok_or_else(|| WriterError::Json("block format is missing its style".into()))?,
    })
}

pub(super) fn revision_json(revision: &RevisionRecord) -> String {
    let end_block_id = revision
        .anchor
        .end_block_id
        .map(|id| format!(r#","end_block_id":{id}"#))
        .unwrap_or_default();
    format!(
        r#"{{"id":{},"author":{},"block_id":{},"start":{},"end":{},"prefix_sha256":{}{},"kind":{},"boundary_kind":{},"before":{},"after":{}}}"#,
        revision.id,
        crate::pkg_json::escape(&revision.author),
        revision.anchor.block_id,
        revision.anchor.start,
        revision.anchor.end,
        crate::pkg_json::escape(&sha256_hex(&revision.anchor.prefix_sha256)),
        end_block_id,
        crate::pkg_json::escape(revision.kind.as_str()),
        crate::pkg_json::escape(revision.boundary_kind.as_str()),
        optional_payload_json(&revision.before),
        optional_payload_json(&revision.after),
    )
}

fn optional_payload_json(payload: &Option<RevisionPayload>) -> String {
    payload.as_ref().map_or_else(|| "null".into(), payload_json)
}

pub(super) fn parse_revision(raw: &str) -> Result<RevisionRecord, WriterError> {
    let mut id = None;
    let mut author = None;
    let mut block_id = None;
    let mut start = None;
    let mut end = None;
    let mut end_block_id = None;
    let mut prefix_sha256 = None;
    let mut kind = None;
    let mut boundary_kind = None;
    let mut before = None;
    let mut after = None;
    let mut before_seen = false;
    let mut after_seen = false;
    let mut seen = HashSet::new();
    for (key, value) in crate::ContentParser::new(raw).parse()? {
        if !seen.insert(key.clone()) {
            return Err(WriterError::Json("duplicate revision field".into()));
        }
        match key.as_str() {
            "id" => id = Some(parse_u64(&value, "bad revision ID")?),
            "author" => author = Some(parse_string(&value, "bad revision author")?),
            "block_id" => block_id = Some(parse_u64(&value, "bad revision block ID")?),
            "start" => start = Some(parse_usize(&value, "bad revision start")?),
            "end" => end = Some(parse_usize(&value, "bad revision end")?),
            "end_block_id" => end_block_id = Some(parse_u64(&value, "bad revision end block ID")?),
            "prefix_sha256" => prefix_sha256 = Some(parse_sha256(&value)?),
            "kind" => {
                let value = parse_string(&value, "bad revision kind")?;
                kind = RevisionKind::parse(&value);
                if kind.is_none() {
                    return Err(WriterError::Json("unknown revision kind".into()));
                }
            }
            "boundary_kind" => {
                let value = parse_string(&value, "bad revision boundary kind")?;
                boundary_kind = RevisionBoundaryKind::parse(&value);
                if boundary_kind.is_none() {
                    return Err(WriterError::Json("unknown revision boundary kind".into()));
                }
            }
            "before" => {
                before_seen = true;
                before = parse_optional_payload(&value)?;
            }
            "after" => {
                after_seen = true;
                after = parse_optional_payload(&value)?;
            }
            _ => {}
        }
    }
    Ok(RevisionRecord {
        id: id.ok_or_else(|| WriterError::Json("revision is missing its ID".into()))?,
        author: author.ok_or_else(|| WriterError::Json("revision is missing its author".into()))?,
        anchor: RevisionAnchor {
            block_id: block_id
                .ok_or_else(|| WriterError::Json("revision is missing its block".into()))?,
            start: start
                .ok_or_else(|| WriterError::Json("revision is missing its start".into()))?,
            end: end.ok_or_else(|| WriterError::Json("revision is missing its end".into()))?,
            end_block_id,
            prefix_sha256: prefix_sha256.ok_or_else(|| {
                WriterError::Json("revision is missing its anchor fingerprint".into())
            })?,
        },
        kind: kind.ok_or_else(|| WriterError::Json("revision is missing its kind".into()))?,
        boundary_kind: boundary_kind.unwrap_or(RevisionBoundaryKind::Unknown),
        before: if before_seen {
            before
        } else {
            return Err(WriterError::Json(
                "revision is missing before payload".into(),
            ));
        },
        after: if after_seen {
            after
        } else {
            return Err(WriterError::Json(
                "revision is missing after payload".into(),
            ));
        },
    })
}

fn parse_optional_payload(
    value: &crate::JsonValue,
) -> Result<Option<RevisionPayload>, WriterError> {
    match value {
        crate::JsonValue::Raw(raw) if raw == "null" => Ok(None),
        crate::JsonValue::Raw(raw) => parse_payload(raw).map(Some),
        _ => Err(WriterError::Json("expected revision payload object".into())),
    }
}

fn parse_payload(raw: &str) -> Result<RevisionPayload, WriterError> {
    let mut text = None;
    let mut runs = None;
    for (key, value) in crate::ContentParser::new(raw).parse()? {
        match key.as_str() {
            "text" if text.is_none() => text = Some(parse_string(&value, "bad revision text")?),
            "runs" if runs.is_none() => {
                let crate::JsonValue::Raw(raw) = value else {
                    return Err(WriterError::Json(
                        "expected revision style runs array".into(),
                    ));
                };
                runs = Some(crate::parse_runs(&raw)?);
            }
            "text" | "runs" => {
                return Err(WriterError::Json("duplicate revision payload field".into()))
            }
            _ => {}
        }
    }
    let payload = RevisionPayload {
        text: text.ok_or_else(|| WriterError::Json("revision payload is missing text".into()))?,
        runs: runs.ok_or_else(|| WriterError::Json("revision payload is missing runs".into()))?,
    };
    payload.validate()?;
    Ok(payload)
}

fn parse_string(value: &crate::JsonValue, error: &str) -> Result<String, WriterError> {
    match value {
        crate::JsonValue::String(value) => Ok(value.clone()),
        _ => Err(WriterError::Json(error.into())),
    }
}

pub(super) fn parse_u64(value: &crate::JsonValue, error: &str) -> Result<u64, WriterError> {
    match value {
        crate::JsonValue::Raw(value) => value.parse().map_err(|_| WriterError::Json(error.into())),
        _ => Err(WriterError::Json(error.into())),
    }
}

fn sha256_hex(digest: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = Vec::with_capacity(64);
    for byte in digest {
        let byte = *byte;
        encoded.push(HEX[(byte >> 4) as usize]);
        encoded.push(HEX[(byte & 0x0f) as usize]);
    }
    String::from_utf8(encoded).expect("hex digits are valid UTF-8")
}

fn parse_sha256(value: &crate::JsonValue) -> Result<[u8; 32], WriterError> {
    let error = "bad revision anchor fingerprint";
    let encoded = parse_string(value, error)?;
    let bytes = encoded.as_bytes();
    if bytes.len() != 64 {
        return Err(WriterError::Json(error.into()));
    }
    let mut digest = [0; 32];
    for (index, pair) in bytes.chunks_exact(2).enumerate() {
        let high = hex_digit(pair[0]).ok_or_else(|| WriterError::Json(error.into()))?;
        let low = hex_digit(pair[1]).ok_or_else(|| WriterError::Json(error.into()))?;
        digest[index] = (high << 4) | low;
    }
    Ok(digest)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn parse_usize(value: &crate::JsonValue, error: &str) -> Result<usize, WriterError> {
    match value {
        crate::JsonValue::Raw(value) => value.parse().map_err(|_| WriterError::Json(error.into())),
        _ => Err(WriterError::Json(error.into())),
    }
}

pub(super) fn payload_bytes(payload: &Option<RevisionPayload>) -> usize {
    payload.as_ref().map_or(0, RevisionPayload::measured_bytes)
}

pub(super) fn payload_capacity(payload: &Option<RevisionPayload>) -> usize {
    payload.as_ref().map_or(0, |payload| {
        payload.text.capacity().saturating_add(
            payload
                .runs
                .capacity()
                .saturating_mul(size_of::<StyleRun>())
                .saturating_add(payload.runs.iter().fold(0usize, |total, run| {
                    total
                        .saturating_add(run.style.font_family.capacity())
                        .saturating_add(run.style.color.as_ref().map_or(0, String::capacity))
                        .saturating_add(run.style.background.as_ref().map_or(0, String::capacity))
                })),
        )
    })
}
