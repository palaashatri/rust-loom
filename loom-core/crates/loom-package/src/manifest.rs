//! Manifest model for Loom document packages.

use std::fmt;

/// The schema version of a manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SchemaVersion {
    /// Major version.
    pub major: u8,
    /// Minor version.
    pub minor: u8,
    /// Patch version.
    pub patch: u8,
}

impl SchemaVersion {
    /// The current schema version.
    pub const CURRENT: SchemaVersion = SchemaVersion::new(0, 1, 0);

    /// Create a new schema version.
    pub const fn new(major: u8, minor: u8, patch: u8) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl core::str::FromStr for SchemaVersion {
    type Err = ManifestError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.split('.');
        let major = parts
            .next()
            .ok_or(ManifestError::InvalidSchema(s.to_string()))?;
        let minor = parts
            .next()
            .ok_or(ManifestError::InvalidSchema(s.to_string()))?;
        let patch = parts
            .next()
            .ok_or(ManifestError::InvalidSchema(s.to_string()))?;
        if parts.next().is_some() {
            return Err(ManifestError::InvalidSchema(s.to_string()));
        }
        Ok(SchemaVersion::new(
            major
                .parse()
                .map_err(|_| ManifestError::InvalidSchema(s.to_string()))?,
            minor
                .parse()
                .map_err(|_| ManifestError::InvalidSchema(s.to_string()))?,
            patch
                .parse()
                .map_err(|_| ManifestError::InvalidSchema(s.to_string()))?,
        ))
    }
}

/// The kind of document a package holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PackageKind {
    /// Writer document.
    Writer,
    /// Sheets workbook.
    Sheets,
    /// Present deck.
    Present,
    /// Photo project.
    Photo,
    /// Motion composition.
    Motion,
    /// Video project.
    Video,
    /// Studio song project.
    Studio,
    /// Encode session.
    Encode,
    /// Vision model pack.
    ModelPack,
    /// Plugin package.
    Plugin,
}

impl PackageKind {
    /// Canonical file extension for this kind.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Writer => "loomdoc",
            Self::Sheets => "loomtable",
            Self::Present => "loomdeck",
            Self::Photo => "loomphoto",
            Self::Motion => "loommotion",
            Self::Video => "loomvideo",
            Self::Studio => "loomstudio",
            Self::Encode => "loomencode",
            Self::ModelPack => "loommodel",
            Self::Plugin => "loomplug",
        }
    }

    /// Canonical MIME type for this kind.
    pub fn mime(self) -> &'static str {
        match self {
            Self::Writer => "application/vnd.loom.document",
            Self::Sheets => "application/vnd.loom.workbook",
            Self::Present => "application/vnd.loom.deck",
            Self::Photo => "application/vnd.loom.photo",
            Self::Motion => "application/vnd.loom.motion",
            Self::Video => "application/vnd.loom.video",
            Self::Studio => "application/vnd.loom.studio",
            Self::Encode => "application/vnd.loom.encode",
            Self::ModelPack => "application/vnd.loom.model",
            Self::Plugin => "application/vnd.loom.plugin",
        }
    }

    /// Parse from a string identifier.
    pub fn from_str_ident(s: &str) -> Option<Self> {
        Some(match s {
            "writer" => Self::Writer,
            "sheets" => Self::Sheets,
            "present" => Self::Present,
            "photo" => Self::Photo,
            "motion" => Self::Motion,
            "video" => Self::Video,
            "studio" => Self::Studio,
            "encode" => Self::Encode,
            "model" => Self::ModelPack,
            "plugin" => Self::Plugin,
            _ => return None,
        })
    }
}

/// Errors produced by manifest parsing or validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    /// JSON syntax error.
    Json(String),
    /// Missing required field.
    MissingField(&'static str),
    /// Invalid schema version string.
    InvalidSchema(String),
    /// Unsupported schema version.
    UnsupportedVersion(SchemaVersion),
    /// Unknown package kind.
    UnknownKind(String),
    /// Invalid MIME type.
    InvalidMime(String),
    /// Invalid checksum hex string.
    InvalidChecksum(String),
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(e) => write!(f, "invalid manifest JSON: {e}"),
            Self::MissingField(fld) => write!(f, "missing required field: {fld}"),
            Self::InvalidSchema(s) => write!(f, "invalid schema version: {s}"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported schema version: {v}"),
            Self::UnknownKind(k) => write!(f, "unknown package kind: {k}"),
            Self::InvalidMime(m) => write!(f, "invalid MIME type: {m}"),
            Self::InvalidChecksum(c) => write!(f, "invalid checksum: {c}"),
        }
    }
}

impl core::error::Error for ManifestError {}

/// A validated MIME type string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MimeType(String);

impl MimeType {
    /// Parse and validate a MIME type string (`type/subtype`).
    pub fn parse(s: &str) -> Result<Self, ManifestError> {
        let (t, sub) = s
            .split_once('/')
            .ok_or_else(|| ManifestError::InvalidMime(s.to_string()))?;
        if t.is_empty() || sub.is_empty() || t.contains(' ') || sub.contains(' ') {
            return Err(ManifestError::InvalidMime(s.to_string()));
        }
        Ok(Self(s.to_string()))
    }

    /// Raw string view.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MimeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// SHA-256 checksum bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Checksum([u8; 32]);

impl Checksum {
    /// Create from raw bytes.
    pub fn from_bytes(b: [u8; 32]) -> Self {
        Self(b)
    }

    /// Parse from lowercase hex.
    pub fn from_hex(s: &str) -> Result<Self, ManifestError> {
        let mut out = [0u8; 32];
        if s.len() != 64 {
            return Err(ManifestError::InvalidChecksum(s.to_string()));
        }
        for (i, ch) in s.chars().enumerate() {
            let v = ch
                .to_digit(16)
                .ok_or_else(|| ManifestError::InvalidChecksum(s.to_string()))?;
            out[i / 2] |= (v as u8) << (if i % 2 == 0 { 4 } else { 0 });
        }
        Ok(Self(out))
    }

    /// Render as lowercase hex.
    pub fn to_hex(&self) -> String {
        let mut s = String::with_capacity(64);
        for b in self.0 {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }

    /// Raw bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// A single entry recorded in the manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestEntry {
    /// Path within the package (normalized, no leading slash).
    pub path: String,
    /// MIME type.
    pub mime: MimeType,
    /// Uncompressed byte length.
    pub size: u64,
    /// SHA-256 of the uncompressed content.
    pub sha256: Checksum,
}

/// The parsed package manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// Schema version.
    pub schema: SchemaVersion,
    /// Package kind.
    pub kind: PackageKind,
    /// Stable document identifier.
    pub id: String,
    /// Human-readable title.
    pub title: String,
    /// Application version that produced this file.
    pub app_version: String,
    /// Entries recorded in content/.
    pub entries: Vec<ManifestEntry>,
}

/// Minimal JSON writer/parser to keep loom-package dependency-free.
///
/// We intentionally keep JSON tiny and validated rather than pulling in a
/// general-purpose parser; schema validation and fuzz safety live here.
pub mod json {
    use super::{
        Checksum, Manifest, ManifestEntry, ManifestError, MimeType, PackageKind, SchemaVersion,
    };

    /// Escape a string for JSON output.
    pub fn escape(s: &str) -> String {
        let mut out = String::with_capacity(s.len() + 2);
        out.push('"');
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }

    /// Write a manifest to JSON.
    pub fn write(manifest: &Manifest) -> String {
        let mut s = String::with_capacity(512);
        s.push('{');
        s.push_str("\"schema\":");
        s.push_str(&format!("\"{}\"", manifest.schema));
        s.push_str(",\"kind\":");
        s.push_str(&escape(kind_ident(manifest.kind)));
        s.push_str(",\"id\":");
        s.push_str(&escape(&manifest.id));
        s.push_str(",\"title\":");
        s.push_str(&escape(&manifest.title));
        s.push_str(",\"appVersion\":");
        s.push_str(&escape(&manifest.app_version));
        s.push_str(",\"entries\":[");
        for (i, e) in manifest.entries.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            s.push('{');
            s.push_str("\"path\":");
            s.push_str(&escape(&e.path));
            s.push_str(",\"mime\":");
            s.push_str(&escape(e.mime.as_str()));
            s.push_str(",\"size\":");
            s.push_str(&escape(&e.size.to_string()));
            s.push_str(",\"sha256\":");
            s.push_str(&escape(&e.sha256.to_hex()));
            s.push('}');
        }
        s.push(']');
        s.push('}');
        s
    }

    /// Very small, safe JSON object parser used strictly for manifests.
    ///
    /// It returns an ordered vector of `(key, value)` pairs where value is a
    /// raw JSON fragment (string, number, array, or nested object). It does
    /// not evaluate expressions or support comments. Depth is bounded.
    pub fn parse_manifest(input: &str) -> Result<Manifest, ManifestError> {
        let mut p = Parser {
            bytes: input.as_bytes(),
            pos: 0,
        };
        p.skip_ws();
        let obj = p.parse_object(0)?;
        drop(obj);
        // Re-parse into a flat map for field lookup.
        let mut p2 = Parser {
            bytes: input.as_bytes(),
            pos: 0,
        };
        p2.skip_ws();
        let fields = p2.parse_object(0)?;

        let schema_str = p2.get_string(&fields, "schema")?;
        let schema: SchemaVersion = schema_str.parse()?;
        if schema > SchemaVersion::CURRENT {
            return Err(ManifestError::UnsupportedVersion(schema));
        }

        let kind = p2.get_string(&fields, "kind")?;
        let kind = PackageKind::from_str_ident(&kind).ok_or(ManifestError::UnknownKind(kind))?;

        let id = p2.get_string(&fields, "id")?;
        let title = p2.get_string(&fields, "title")?;
        let app_version = p2.get_string(&fields, "appVersion")?;

        let entries = p2.get_array(&fields, "entries")?;
        let mut parsed = Vec::with_capacity(entries.len());
        for e in &entries {
            let o = p2.as_object(e)?;
            let path = p2.get_string(o, "path")?;
            let mime = MimeType::parse(&p2.get_string(o, "mime")?)?;
            let size: u64 = p2
                .get_string(o, "size")?
                .parse()
                .map_err(|_| ManifestError::Json("invalid size".to_string()))?;
            let sha256 = Checksum::from_hex(&p2.get_string(o, "sha256")?)?;
            parsed.push(ManifestEntry {
                path,
                mime,
                size,
                sha256,
            });
        }

        Ok(Manifest {
            schema,
            kind,
            id,
            title,
            app_version,
            entries: parsed,
        })
    }

    fn kind_ident(k: PackageKind) -> &'static str {
        match k {
            PackageKind::Writer => "writer",
            PackageKind::Sheets => "sheets",
            PackageKind::Present => "present",
            PackageKind::Photo => "photo",
            PackageKind::Motion => "motion",
            PackageKind::Video => "video",
            PackageKind::Studio => "studio",
            PackageKind::Encode => "encode",
            PackageKind::ModelPack => "model",
            PackageKind::Plugin => "plugin",
        }
    }

    type FieldList = Vec<(String, Value)>;

    #[derive(Debug, Clone)]
    enum Value {
        Str(String),
        #[allow(dead_code)]
        Num(f64),
        #[allow(dead_code)]
        Bool(bool),
        Null,
        #[allow(dead_code)]
        Arr(Vec<Value>),
        #[allow(dead_code)]
        Obj(FieldList),
    }

    struct Parser<'a> {
        bytes: &'a [u8],
        pos: usize,
    }

    impl<'a> Parser<'a> {
        fn skip_ws(&mut self) {
            while self.pos < self.bytes.len()
                && matches!(self.bytes[self.pos], b' ' | b'\t' | b'\n' | b'\r')
            {
                self.pos += 1;
            }
        }

        fn peek(&self) -> Option<u8> {
            self.bytes.get(self.pos).copied()
        }

        fn next(&mut self) -> Option<u8> {
            let b = self.peek();
            if b.is_some() {
                self.pos += 1;
            }
            b
        }

        fn expect(&mut self, b: u8) -> Result<(), ManifestError> {
            if self.peek() == Some(b) {
                self.pos += 1;
                Ok(())
            } else {
                Err(ManifestError::Json(format!(
                    "expected '{}' at byte {}",
                    b as char, self.pos
                )))
            }
        }

        fn parse_value(&mut self, depth: usize) -> Result<Value, ManifestError> {
            if depth > 32 {
                return Err(ManifestError::Json("nesting too deep".to_string()));
            }
            self.skip_ws();
            match self.peek() {
                Some(b'{') => self.parse_object(depth).map(Value::Obj),
                Some(b'[') => self.parse_array(depth).map(Value::Arr),
                Some(b'"') => self.parse_string().map(Value::Str),
                Some(b't') => {
                    self.expect(b't')?;
                    self.expect(b'r')?;
                    self.expect(b'u')?;
                    self.expect(b'e')?;
                    Ok(Value::Bool(true))
                }
                Some(b'f') => {
                    self.expect(b'f')?;
                    self.expect(b'a')?;
                    self.expect(b'l')?;
                    self.expect(b's')?;
                    self.expect(b'e')?;
                    Ok(Value::Bool(false))
                }
                Some(b'n') => {
                    self.expect(b'n')?;
                    self.expect(b'u')?;
                    self.expect(b'l')?;
                    self.expect(b'l')?;
                    Ok(Value::Null)
                }
                Some(_) => self.parse_number().map(Value::Num),
                None => Err(ManifestError::Json("unexpected end of input".to_string())),
            }
        }

        fn parse_number(&mut self) -> Result<f64, ManifestError> {
            let start = self.pos;
            while self.pos < self.bytes.len()
                && matches!(
                    self.bytes[self.pos],
                    b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E'
                )
            {
                self.pos += 1;
            }
            let s = core::str::from_utf8(&self.bytes[start..self.pos])
                .map_err(|_| ManifestError::Json("invalid number".to_string()))?;
            s.parse()
                .map_err(|_| ManifestError::Json("invalid number".to_string()))
        }

        fn parse_string(&mut self) -> Result<String, ManifestError> {
            self.expect(b'"')?;
            let mut out = String::new();
            loop {
                let b = self
                    .next()
                    .ok_or_else(|| ManifestError::Json("unterminated string".to_string()))?;
                match b {
                    b'"' => break,
                    b'\\' => {
                        let esc = self
                            .next()
                            .ok_or_else(|| ManifestError::Json("bad escape".to_string()))?;
                        match esc {
                            b'"' => out.push('"'),
                            b'\\' => out.push('\\'),
                            b'/' => out.push('/'),
                            b'b' => out.push('\u{0008}'),
                            b'f' => out.push('\u{000C}'),
                            b'n' => out.push('\n'),
                            b'r' => out.push('\r'),
                            b't' => out.push('\t'),
                            b'u' => {
                                let mut cp = 0u32;
                                for _ in 0..4 {
                                    let h = self.next().ok_or_else(|| {
                                        ManifestError::Json("bad unicode".to_string())
                                    })?;
                                    let v = match h {
                                        b'0'..=b'9' => h - b'0',
                                        b'a'..=b'f' => h - b'a' + 10,
                                        b'A'..=b'F' => h - b'A' + 10,
                                        _ => {
                                            return Err(ManifestError::Json(
                                                "bad unicode".to_string(),
                                            ))
                                        }
                                    };
                                    cp = (cp << 4) | v as u32;
                                }
                                if let Some(c) = char::from_u32(cp) {
                                    out.push(c);
                                }
                            }
                            _ => return Err(ManifestError::Json("bad escape".to_string())),
                        }
                    }
                    b if b < 0x20 => {
                        return Err(ManifestError::Json("control char in string".to_string()))
                    }
                    _ => {
                        // Assemble UTF-8 bytes.
                        let mut seq = vec![b];
                        let mut needed = if b >= 0xF0 {
                            3
                        } else if b >= 0xE0 {
                            2
                        } else if b >= 0xC0 {
                            1
                        } else {
                            0
                        };
                        while needed > 0 {
                            let nb = self
                                .next()
                                .ok_or_else(|| ManifestError::Json("truncated utf8".to_string()))?;
                            seq.push(nb);
                            needed -= 1;
                        }
                        let s = core::str::from_utf8(&seq)
                            .map_err(|_| ManifestError::Json("invalid utf8".to_string()))?;
                        out.push_str(s);
                    }
                }
            }
            Ok(out)
        }

        fn parse_array(&mut self, depth: usize) -> Result<Vec<Value>, ManifestError> {
            self.expect(b'[')?;
            let mut items = Vec::new();
            self.skip_ws();
            if self.peek() == Some(b']') {
                self.pos += 1;
                return Ok(items);
            }
            loop {
                items.push(self.parse_value(depth + 1)?);
                self.skip_ws();
                match self.peek() {
                    Some(b',') => {
                        self.pos += 1;
                    }
                    Some(b']') => {
                        self.pos += 1;
                        break;
                    }
                    _ => return Err(ManifestError::Json("expected ',' or ']'".to_string())),
                }
            }
            Ok(items)
        }

        fn parse_object(&mut self, depth: usize) -> Result<FieldList, ManifestError> {
            self.expect(b'{')?;
            let mut fields = Vec::new();
            self.skip_ws();
            if self.peek() == Some(b'}') {
                self.pos += 1;
                return Ok(fields);
            }
            loop {
                self.skip_ws();
                let key = self.parse_string()?;
                self.skip_ws();
                self.expect(b':')?;
                let val = self.parse_value(depth + 1)?;
                fields.push((key, val));
                self.skip_ws();
                match self.peek() {
                    Some(b',') => {
                        self.pos += 1;
                    }
                    Some(b'}') => {
                        self.pos += 1;
                        break;
                    }
                    _ => return Err(ManifestError::Json("expected ',' or '}'".to_string())),
                }
            }
            Ok(fields)
        }

        fn get_string(&self, fields: &FieldList, key: &str) -> Result<String, ManifestError> {
            for (k, v) in fields {
                if k == key {
                    if let Value::Str(s) = v {
                        return Ok(s.clone());
                    }
                    return Err(ManifestError::Json(format!("field '{key}' not a string")));
                }
            }
            Err(ManifestError::MissingField("missing field"))
        }

        fn get_array(&self, fields: &FieldList, key: &str) -> Result<Vec<Value>, ManifestError> {
            for (k, v) in fields {
                if k == key {
                    if let Value::Arr(a) = v {
                        return Ok(a.clone());
                    }
                    return Err(ManifestError::Json(format!("field '{key}' not an array")));
                }
            }
            Err(ManifestError::Json(format!("missing array field '{key}'")))
        }

        fn as_object<'b>(&self, v: &'b Value) -> Result<&'b FieldList, ManifestError> {
            if let Value::Obj(o) = v {
                Ok(o)
            } else {
                Err(ManifestError::Json("expected object".to_string()))
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn sample() -> Manifest {
            Manifest {
                schema: SchemaVersion::CURRENT,
                kind: PackageKind::Writer,
                id: "doc-0001".into(),
                title: "Hello".into(),
                app_version: "0.1.0".into(),
                entries: vec![ManifestEntry {
                    path: "content/document.json".into(),
                    mime: MimeType::parse("application/json").unwrap(),
                    size: 42,
                    sha256: Checksum::from_bytes([0xAB; 32]),
                }],
            }
        }

        #[test]
        fn round_trip() {
            let m = sample();
            let s = write(&m);
            let parsed = parse_manifest(&s).unwrap();
            assert_eq!(parsed, m);
        }

        #[test]
        fn rejects_bad_schema() {
            let m = sample();
            let mut s = write(&m);
            s = s.replace("0.1.0", "99.0.0");
            assert!(parse_manifest(&s).is_err());
        }

        #[test]
        fn rejects_garbage() {
            assert!(parse_manifest("not json").is_err());
            assert!(parse_manifest("{}").is_err());
        }

        #[test]
        fn rejects_deep_nesting() {
            let deep = format!("{}[{}]", "[".repeat(40), "]".repeat(40));
            let wrapped = format!(
                "{{\"a\":{},\"schema\":\"0.1.0\",\"kind\":\"writer\",\"id\":\"x\",\"title\":\"t\",\"appVersion\":\"0.1.0\",\"entries\":[]}}",
                deep
            );
            assert!(parse_manifest(&wrapped).is_err());
        }

        #[test]
        fn rejects_unescaped_control() {
            let s = "{\"schema\":\"0.1.0\",\"kind\":\"writer\",\"id\":\"x\n\",\"title\":\"t\",\"appVersion\":\"0.1.0\",\"entries\":[]}";
            assert!(parse_manifest(s).is_err());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_version_parse() {
        assert_eq!(
            "0.1.0".parse::<SchemaVersion>().unwrap(),
            SchemaVersion::new(0, 1, 0)
        );
        assert_eq!(SchemaVersion::new(0, 1, 0).to_string(), "0.1.0");
        assert!("1.2".parse::<SchemaVersion>().is_err());
        assert!("a.b.c".parse::<SchemaVersion>().is_err());
    }

    #[test]
    fn extension_and_mime() {
        assert_eq!(PackageKind::Writer.extension(), "loomdoc");
        assert_eq!(PackageKind::Sheets.extension(), "loomtable");
        assert_eq!(PackageKind::Writer.mime(), "application/vnd.loom.document");
    }

    #[test]
    fn checksum_hex_roundtrip() {
        let c = Checksum::from_hex(&"ab".repeat(32)).unwrap();
        assert_eq!(c.to_hex(), "ab".repeat(32));
        assert!(Checksum::from_hex(&"zz".repeat(32)).is_err());
        assert!(Checksum::from_hex("ab").is_err());
    }

    #[test]
    fn mime_validation() {
        assert!(MimeType::parse("application/json").is_ok());
        assert!(MimeType::parse("application/json utf8").is_err());
        assert!(MimeType::parse("application").is_err());
        assert!(MimeType::parse("").is_err());
    }
}
