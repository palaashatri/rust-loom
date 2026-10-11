//! The fonts a Writer document can name, and how this machine meets them.
//!
//! A character style stores the family *name* the user chose, never a face
//! handle or the stand-in this machine happened to pick: the file must mean the
//! same thing on a machine that has the font. Everything that measures or
//! draws text asks this module which family to use:
//!
//! * One process-wide [`FontCatalog`] holds the installed and bundled faces.
//!   It starts as the bundled Inter faces, so the editor works at once, and an
//!   application swaps in the system catalogue with [`install_font_catalog`]
//!   when its background scan finishes. Every swap advances [`font_epoch`], so
//!   caches that depend on the catalogue know to start over.
//! * [`resolve_family`] answers which concrete family is drawn for a stored
//!   name, and whether it is the family itself or a stand-in.
//! * The names Loom's own earlier three-way control stored ("Sans", "Serif",
//!   "Monospace") are generic: "Sans" is the bundled document font, "Serif" and
//!   "Monospace" are the system's serif and monospace choices, resolved the
//!   same way a CSS generic family is.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

pub use loom_fonts::{Availability, FontCatalog, ScanCache, ScanConfig, ScanLimits, ScanReport};

use crate::WriterDocument;

/// The family text with no family of its own is set in: the bundled face the
/// window shell draws documents with.
pub const DEFAULT_FAMILY: &str = "Inter";

static CATALOG: RwLock<Option<Arc<FontCatalog>>> = RwLock::new(None);
static EPOCH: AtomicU64 = AtomicU64::new(0);

/// The catalogue measurement and drawing choices are made against.
pub fn font_catalog() -> Arc<FontCatalog> {
    if let Some(installed) = CATALOG.read().ok().and_then(|slot| slot.clone()) {
        return installed;
    }
    static BUNDLED: OnceLock<Arc<FontCatalog>> = OnceLock::new();
    BUNDLED
        .get_or_init(|| Arc::new(FontCatalog::bundled_only()))
        .clone()
}

/// Replaces the catalogue, for example with the result of a system scan.
/// Layout and measurement caches that depend on it are rebuilt on next use.
pub fn install_font_catalog(catalog: Arc<FontCatalog>) {
    if let Ok(mut slot) = CATALOG.write() {
        *slot = Some(catalog);
    }
    EPOCH.fetch_add(1, Ordering::SeqCst);
}

/// Returns to the bundled-only catalogue (tests, and a failed scan).
pub fn reset_font_catalog() {
    if let Ok(mut slot) = CATALOG.write() {
        *slot = None;
    }
    EPOCH.fetch_add(1, Ordering::SeqCst);
}

/// A counter that changes whenever [`install_font_catalog`] or
/// [`reset_font_catalog`] runs.
pub fn font_epoch() -> u64 {
    EPOCH.load(Ordering::SeqCst)
}

/// The lookup key of a stored family name, or `None` for the document font
/// (an unnamed family, Inter itself, and Loom's own "Sans").
pub(crate) fn family_key(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_lowercase();
    match lower.as_str() {
        "sans" | "inter" => None,
        "mono" | "monospace" => Some("monospace".to_owned()),
        "serif" => Some("serif".to_owned()),
        _ => Some(lower),
    }
}

/// True when `name` draws in the document font (Inter), whatever the system
/// has installed.
pub fn is_document_font(name: &str) -> bool {
    family_key(name).is_none()
}

/// True when two stored names mean the same family: the same name apart from
/// case and surrounding space, or both the document font.
pub fn same_family(left: &str, right: &str) -> bool {
    family_key(left) == family_key(right)
}

/// How a stored family name stands on this machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FamilyStanding {
    /// The bundled document font.
    DocumentFont,
    /// Installed (or bundled) under that name.
    Installed,
    /// A generic name such as "Serif", met by whatever the system offers.
    Generic,
    /// Not installed, but the stand-in has the same advances.
    MetricCompatible,
    /// Not installed: text is drawn and measured in the stand-in.
    NotInstalled,
}

impl FamilyStanding {
    /// True when the user should be told the font is not installed.
    pub fn is_missing(self) -> bool {
        self == Self::NotInstalled
    }
}

/// The concrete family behind a stored name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FamilyResolution {
    /// The name as stored in the document (empty for none).
    pub requested: String,
    /// The installed or bundled family that is drawn and measured. Hand this
    /// name, not `requested`, to the text renderer.
    pub drawn: String,
    /// How `requested` stands against what is installed.
    pub standing: FamilyStanding,
}

/// Resolves a stored family name against the current catalogue.
pub fn resolve_family(requested: &str) -> FamilyResolution {
    let Some(key) = family_key(requested) else {
        return FamilyResolution {
            requested: requested.to_owned(),
            drawn: DEFAULT_FAMILY.to_owned(),
            standing: FamilyStanding::DocumentFont,
        };
    };
    let catalog = font_catalog();
    let standing = match catalog.availability(&key) {
        Availability::Installed | Availability::Bundled => FamilyStanding::Installed,
        Availability::Generic => FamilyStanding::Generic,
        Availability::MetricCompatible { .. } => FamilyStanding::MetricCompatible,
        Availability::Missing { .. } => FamilyStanding::NotInstalled,
    };
    let drawn = catalog
        .resolve(&key, 400, false)
        .ok()
        .and_then(|font| catalog.face(font.primary()).map(|face| face.family.clone()))
        .unwrap_or_else(|| DEFAULT_FAMILY.to_owned());
    FamilyResolution {
        requested: requested.to_owned(),
        drawn,
        standing,
    }
}

/// The families the document's character runs name, in order of first use,
/// without repeats (case-insensitive). The document font is not listed.
pub fn document_families(document: &WriterDocument) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    let mut families: Vec<String> = Vec::new();
    for run in document.blocks.iter().flat_map(|block| block.runs.iter()) {
        let Some(key) = family_key(&run.style.font_family) else {
            continue;
        };
        if !seen.contains(&key) {
            seen.push(key);
            families.push(run.style.font_family.trim().to_owned());
        }
    }
    families
}

/// The families a PDF export would draw in Inter because it cannot embed them
/// yet: every family the runs name other than the document font itself.
pub fn pdf_substituted_families(document: &WriterDocument) -> Vec<String> {
    document_families(document)
}

/// The sentence the status line shows after a PDF export that substituted
/// fonts, or `None` when every run is set in the document font.
pub fn pdf_substitution_note(document: &WriterDocument) -> Option<String> {
    let families = pdf_substituted_families(document);
    if families.is_empty() {
        return None;
    }
    Some(format!(
        "PDF export used {} for {} (embedding not supported yet)",
        DEFAULT_FAMILY,
        families.join(", ")
    ))
}

/// Scans the system's font directories with `limits`, reusing the per-file
/// results `cache_text` holds (the text form of a [`ScanCache`]). Returns the
/// catalogue and the cache text to store for the next run.
///
/// This does file I/O for as long as the scan takes (bounded by `limits`), so
/// an application calls it on a worker thread.
pub fn scan_system_fonts(config: &ScanConfig, cache_text: Option<&str>) -> (FontCatalog, String) {
    let mut cache = cache_text.map_or_else(ScanCache::new, ScanCache::from_text);
    let catalog = FontCatalog::scan_with_cache(config, &mut cache);
    (catalog, cache.to_text())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fonts;

    #[test]
    fn the_unnamed_family_inter_and_the_old_sans_are_the_document_font() {
        for name in ["", "  ", "Inter", "inter", "Sans", "sans"] {
            let resolved = resolve_family(name);
            assert_eq!(resolved.standing, FamilyStanding::DocumentFont, "{name:?}");
            assert_eq!(resolved.drawn, "Inter", "{name:?}");
            assert!(is_document_font(name));
        }
    }

    #[test]
    fn serif_and_monospace_resolve_to_the_installed_concrete_family() {
        let _fonts = test_fonts::install(&[("Georgia", 100), ("Consolas", 100)]);
        for (name, expected) in [
            ("Serif", "Georgia"),
            ("serif", "Georgia"),
            ("Monospace", "Consolas"),
            ("Mono", "Consolas"),
        ] {
            let resolved = resolve_family(name);
            assert_eq!(resolved.standing, FamilyStanding::Generic, "{name:?}");
            assert_eq!(resolved.drawn, expected, "{name:?}");
        }
    }

    #[test]
    fn a_generic_name_with_nothing_installed_still_names_a_real_family() {
        let _fonts = test_fonts::install(&[]);
        for name in ["Serif", "Monospace"] {
            let resolved = resolve_family(name);
            assert_eq!(resolved.standing, FamilyStanding::Generic);
            assert!(font_catalog().has_family(&resolved.drawn), "{resolved:?}");
        }
    }

    #[test]
    fn a_missing_family_keeps_its_name_and_draws_in_a_stand_in() {
        let _fonts = test_fonts::install(&[]);
        let resolved = resolve_family("Zzyzx Display");
        assert_eq!(resolved.requested, "Zzyzx Display");
        assert_eq!(resolved.standing, FamilyStanding::NotInstalled);
        assert!(resolved.standing.is_missing());
        assert!(font_catalog().has_family(&resolved.drawn));
    }

    #[test]
    fn an_installed_family_is_drawn_as_itself() {
        let _fonts = test_fonts::install(&[("Wider", 200)]);
        let resolved = resolve_family("wider");
        assert_eq!(resolved.standing, FamilyStanding::Installed);
        assert_eq!(resolved.drawn, "Wider");
    }

    #[test]
    fn a_metric_compatible_stand_in_is_not_reported_missing() {
        let _fonts = test_fonts::install(&[("Liberation Sans", 100)]);
        let resolved = resolve_family("Arial");
        assert_eq!(resolved.standing, FamilyStanding::MetricCompatible);
        assert!(!resolved.standing.is_missing());
        assert_eq!(resolved.drawn, "Liberation Sans");
    }

    #[test]
    fn families_in_use_are_listed_once_in_first_use_order() {
        use loom_text::{CharacterStyle, StyleRun};
        let mut document = WriterDocument::new("families", "Families");
        let mut block = crate::RichBlock::new(1, "paragraph", "alpha beta gamma delta");
        let styled = |family: &str| CharacterStyle {
            font_family: family.to_owned(),
            ..CharacterStyle::default()
        };
        block.runs = vec![
            StyleRun {
                start: 0,
                end: 5,
                style: styled("Georgia"),
            },
            StyleRun {
                start: 6,
                end: 10,
                style: styled("Sans"),
            },
            StyleRun {
                start: 11,
                end: 16,
                style: styled("georgia"),
            },
            StyleRun {
                start: 17,
                end: 22,
                style: styled("Garamond"),
            },
        ];
        document.push(block);
        assert_eq!(document_families(&document), ["Georgia", "Garamond"]);
        assert_eq!(
            pdf_substitution_note(&document).as_deref(),
            Some("PDF export used Inter for Georgia, Garamond (embedding not supported yet)")
        );
        assert_eq!(
            pdf_substitution_note(&WriterDocument::new("plain", "Plain")),
            None
        );
    }

    #[test]
    fn a_family_survives_saving_and_reopening_the_document() {
        use loom_text::{CharacterStyle, StyleRun};
        let mut document = WriterDocument::new("persist", "Persist");
        let mut block = crate::RichBlock::new(1, "paragraph", "Set in Garamond Pro");
        block.runs = vec![StyleRun {
            start: 7,
            end: 19,
            style: CharacterStyle {
                font_family: "Garamond Pro".to_owned(),
                ..CharacterStyle::default()
            },
        }];
        document.push(block);
        let bytes = crate::save_document(&document).expect("save");
        let reopened = crate::load_document(&bytes).expect("load");
        assert_eq!(reopened.blocks[0].runs.len(), 1);
        assert_eq!(reopened.blocks[0].runs[0].style.font_family, "Garamond Pro");
        assert_eq!(document_families(&reopened), ["Garamond Pro"]);
    }

    #[test]
    fn installing_a_catalogue_advances_the_epoch_and_a_scan_reuses_its_cache() {
        let before = font_epoch();
        let fonts = test_fonts::install(&[("Wider", 200)]);
        assert!(font_epoch() > before);
        let config = fonts.scan_config();
        let (first, cache_text) = scan_system_fonts(&config, None);
        assert!(first.has_family("Wider"));
        assert_eq!(first.report().cache_hits, 0);
        let (second, _) = scan_system_fonts(&config, Some(&cache_text));
        assert!(second.has_family("Wider"));
        assert!(
            second.report().cache_hits > 0,
            "the second scan reads no file"
        );
    }
}
