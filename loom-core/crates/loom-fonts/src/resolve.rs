//! Choosing a face for (family, weight, italic) and building its fallback
//! chain. Pure functions over small candidate lists so the rules are testable
//! without any font on disk.

use crate::info::FaceId;

/// The facts about a face that matching looks at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub weight: u16,
    pub width: u16,
    pub italic: bool,
}

/// The outcome of matching one family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Match {
    /// Index into the candidate slice.
    pub index: usize,
    /// The request wants bold (weight 600 or more) but the face is lighter.
    pub synthetic_bold: bool,
    /// The request wants italic but the family has only upright faces.
    pub synthetic_italic: bool,
}

/// Picks the best of `candidates` for the request.
///
/// The order follows CSS Fonts: width nearest normal first, then style, then
/// weight. Weight 400 to 500 prefers the other of the pair, then lighter
/// weights, then heavier ones; below 400 prefers lighter then heavier; above
/// 500 prefers heavier then lighter. Ties go to the earliest candidate, so a
/// sorted catalogue gives a deterministic answer.
pub(crate) fn best_match(candidates: &[Candidate], weight: u16, italic: bool) -> Option<Match> {
    if candidates.is_empty() {
        return None;
    }
    let weight = weight.clamp(1, 1000);
    let target_width = candidates
        .iter()
        .map(|c| c.width)
        .min_by_key(|w| ((i32::from(*w) - 5).abs(), *w))?;
    let by_width: Vec<usize> = (0..candidates.len())
        .filter(|&i| candidates[i].width == target_width)
        .collect();

    let styled: Vec<usize> = by_width
        .iter()
        .copied()
        .filter(|&i| candidates[i].italic == italic)
        .collect();
    let (pool, synthetic_italic) = if styled.is_empty() {
        (by_width, italic)
    } else {
        (styled, false)
    };

    let index = pick_weight(candidates, &pool, weight)?;
    Some(Match {
        index,
        synthetic_bold: weight >= 600 && candidates[index].weight < 600,
        synthetic_italic,
    })
}

fn pick_weight(candidates: &[Candidate], pool: &[usize], desired: u16) -> Option<usize> {
    let weight = |i: usize| candidates[i].weight;
    let exact = pool.iter().copied().find(|&i| weight(i) == desired);
    if exact.is_some() {
        return exact;
    }
    // Nearest below (largest weight under desired) and nearest above.
    let below = |limit: u16| {
        pool.iter()
            .copied()
            .filter(|&i| weight(i) < limit)
            .max_by_key(|&i| (weight(i), std::cmp::Reverse(i)))
    };
    let above = |floor: u16, ceiling: u16| {
        pool.iter()
            .copied()
            .filter(|&i| weight(i) > floor && weight(i) <= ceiling)
            .min_by_key(|&i| (weight(i), i))
    };
    if desired < 400 {
        below(desired).or_else(|| above(desired, u16::MAX))
    } else if desired > 500 {
        above(desired, u16::MAX).or_else(|| below(desired))
    } else {
        above(desired, 500)
            .or_else(|| below(desired))
            .or_else(|| above(500, u16::MAX))
    }
}

/// An ordered list of families tried, after the requested one, when a
/// character or the whole family is unavailable. The bundled Inter face is
/// always appended by the catalogue, so a chain never runs dry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FallbackPolicy {
    families: Vec<String>,
}

impl FallbackPolicy {
    /// A policy trying exactly `families`, in order.
    pub fn new<I, S>(families: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            families: families.into_iter().map(Into::into).collect(),
        }
    }

    /// The families a desktop of the running operating system most likely has.
    pub fn platform_default() -> Self {
        let list: &[&str] = if cfg!(windows) {
            &["Segoe UI", "Arial", "Microsoft YaHei", "Segoe UI Symbol"]
        } else if cfg!(target_os = "macos") {
            &["Helvetica Neue", "Arial", "PingFang SC", "Apple Symbols"]
        } else {
            &[
                "DejaVu Sans",
                "Liberation Sans",
                "Noto Sans",
                "Noto Sans CJK SC",
            ]
        };
        Self::new(list.iter().copied())
    }

    /// The families, in the order tried.
    pub fn families(&self) -> &[String] {
        &self.families
    }
}

impl Default for FallbackPolicy {
    fn default() -> Self {
        Self::platform_default()
    }
}

/// Families that stand for a generic style rather than an installed font.
pub(crate) fn generic_chain(normalized: &str) -> Option<&'static [&'static str]> {
    match normalized {
        "sans-serif" | "system-ui" | "ui-sans-serif" => Some(&[
            "segoe ui",
            "helvetica neue",
            "arial",
            "liberation sans",
            "dejavu sans",
            "noto sans",
        ]),
        "serif" | "ui-serif" => Some(&[
            "times new roman",
            "georgia",
            "liberation serif",
            "dejavu serif",
            "noto serif",
        ]),
        "monospace" | "ui-monospace" => Some(&[
            "consolas",
            "menlo",
            "courier new",
            "liberation mono",
            "dejavu sans mono",
            "noto sans mono",
        ]),
        _ => None,
    }
}

/// Free families with the same glyph advances as a common proprietary one, so
/// a document set in Arial keeps its line breaks where Arial is absent.
pub(crate) fn metric_compatible(normalized: &str) -> &'static [&'static str] {
    match normalized {
        "arial" => &["liberation sans", "arimo", "nimbus sans"],
        "helvetica" | "helvetica neue" => &["arial", "liberation sans", "arimo", "nimbus sans"],
        "times new roman" | "times" => &["liberation serif", "tinos", "nimbus roman"],
        "courier new" | "courier" => &["liberation mono", "cousine", "nimbus mono ps"],
        "calibri" => &["carlito"],
        "cambria" => &["caladea"],
        _ => &[],
    }
}

/// A resolved font request: the face to measure with and what to try when it
/// lacks a character.
///
/// The request itself (`requested_*`) is what a document stores; the chain is
/// derived and must be recomputed after the catalogue changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontRef {
    /// The family name exactly as the document asked for it.
    pub requested_family: String,
    /// The requested CSS weight.
    pub requested_weight: u16,
    /// The requested italic flag.
    pub requested_italic: bool,
    /// Faces to try in order; the first is the one metrics come from.
    pub chain: Vec<FaceId>,
    /// True when the requested family is not installed and `chain[0]` is a
    /// stand-in. The document keeps the requested name.
    pub substituted: bool,
    /// True when the stand-in is a family known to have identical advances
    /// (Liberation Sans for Arial); line breaks will not move.
    pub metric_compatible: bool,
    /// The face is lighter than the requested bold; a renderer may embolden.
    pub synthetic_bold: bool,
    /// The family has no italic face; a renderer may slant.
    pub synthetic_italic: bool,
}

impl FontRef {
    /// The face metrics come from.
    pub fn primary(&self) -> FaceId {
        self.chain[0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upright(weights: &[u16]) -> Vec<Candidate> {
        weights
            .iter()
            .map(|&weight| Candidate {
                weight,
                width: 5,
                italic: false,
            })
            .collect()
    }

    fn pick(weights: &[u16], desired: u16) -> u16 {
        let candidates = upright(weights);
        let found = best_match(&candidates, desired, false).unwrap();
        candidates[found.index].weight
    }

    #[test]
    fn exact_weight_wins() {
        assert_eq!(pick(&[300, 400, 500, 700], 500), 500);
    }

    #[test]
    fn regular_prefers_medium_then_lighter_then_heavier() {
        assert_eq!(pick(&[300, 500, 700], 400), 500);
        assert_eq!(pick(&[300, 700], 400), 300);
        assert_eq!(pick(&[600, 700], 400), 600);
        assert_eq!(pick(&[300, 400], 450), 400);
    }

    #[test]
    fn light_requests_prefer_lighter_then_heavier() {
        assert_eq!(pick(&[100, 200, 400], 300), 200);
        assert_eq!(pick(&[400, 700], 300), 400);
    }

    #[test]
    fn bold_requests_prefer_heavier_then_lighter() {
        assert_eq!(pick(&[400, 700, 900], 800), 900);
        assert_eq!(pick(&[400, 500], 700), 500);
    }

    #[test]
    fn italic_is_preferred_and_synthesised_only_when_missing() {
        let candidates = vec![
            Candidate {
                weight: 400,
                width: 5,
                italic: false,
            },
            Candidate {
                weight: 400,
                width: 5,
                italic: true,
            },
        ];
        let italic = best_match(&candidates, 400, true).unwrap();
        assert_eq!((italic.index, italic.synthetic_italic), (1, false));
        let upright_only = best_match(&candidates[..1], 400, true).unwrap();
        assert!(upright_only.synthetic_italic);
        let italic_only = best_match(&candidates[1..], 400, false).unwrap();
        assert_eq!(
            (italic_only.index, italic_only.synthetic_italic),
            (0, false)
        );
    }

    #[test]
    fn bold_is_synthesised_only_when_every_face_is_light() {
        let candidates = upright(&[400]);
        assert!(best_match(&candidates, 700, false).unwrap().synthetic_bold);
        assert!(!best_match(&candidates, 400, false).unwrap().synthetic_bold);
        let with_bold = upright(&[400, 700]);
        assert!(!best_match(&with_bold, 700, false).unwrap().synthetic_bold);
    }

    #[test]
    fn normal_width_is_preferred_to_condensed() {
        let candidates = vec![
            Candidate {
                weight: 400,
                width: 3,
                italic: false,
            },
            Candidate {
                weight: 400,
                width: 5,
                italic: false,
            },
        ];
        assert_eq!(best_match(&candidates, 400, false).unwrap().index, 1);
    }

    #[test]
    fn empty_input_has_no_match() {
        assert!(best_match(&[], 400, false).is_none());
    }

    #[test]
    fn generic_and_metric_tables_are_lower_case() {
        for name in ["sans-serif", "serif", "monospace"] {
            assert!(generic_chain(name)
                .unwrap()
                .iter()
                .all(|f| *f == f.to_lowercase()));
        }
        assert_eq!(metric_compatible("arial")[0], "liberation sans");
        assert!(metric_compatible("comic sans ms").is_empty());
    }
}
