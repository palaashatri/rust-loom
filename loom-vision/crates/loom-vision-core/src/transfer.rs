//! Cross-application transfer encodings for Vision outputs.
//!
//! Applications consume Vision results as ordinary editable data. This module defines
//! the interchange vocabulary: OCR text blocks carrying source-region provenance, and a
//! deterministic run-length codec for compacting segmentation masks in transit.

use crate::error::VisionError;

/// One recognized text block paired with its source region and confidence.
///
/// Applications convert these blocks into editable paragraphs, table ranges, or captions
/// while retaining the region so results stay traceable to the scanned source.
#[derive(Debug, Clone, PartialEq)]
pub struct OcrTextBlock {
    /// Recognized text content.
    pub text: String,
    /// Source region `(x, y, width, height)` in source-image pixels.
    pub region: (u32, u32, u32, u32),
    /// Recognition confidence in `[0, 1]`.
    pub confidence: f32,
}

impl OcrTextBlock {
    /// Creates a block, validating the payload.
    pub fn new(
        text: impl Into<String>,
        region: (u32, u32, u32, u32),
        confidence: f32,
    ) -> Result<Self, VisionError> {
        let text = text.into();
        if text.trim().is_empty() {
            return Err(VisionError::Internal(
                "OCR text block must not be empty".into(),
            ));
        }
        if !(0.0..=1.0).contains(&confidence) {
            return Err(VisionError::Internal(format!(
                "OCR confidence {confidence} must be within [0, 1]"
            )));
        }
        let (_x, _y, w, h) = region;
        if w == 0 || h == 0 {
            return Err(VisionError::Internal(
                "OCR region must have positive extent".into(),
            ));
        }
        Ok(Self {
            text,
            region,
            confidence,
        })
    }

    /// Orders blocks for reading flow: top-to-bottom, then left-to-right.
    pub fn reading_order(a: &Self, b: &Self) -> std::cmp::Ordering {
        (a.region.1, a.region.0).cmp(&(b.region.1, b.region.0))
    }
}

/// One run of equal mask bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaskSpan {
    /// Byte value repeated by this span.
    pub value: u8,
    /// Number of pixels covered.
    pub length: u32,
}

/// Encodes a row-major mask into deterministic runs of equal bytes. An empty mask yields
/// an empty span list; the encoding always alternates values and preserves total coverage.
pub fn mask_to_spans(mask: &[u8]) -> Vec<MaskSpan> {
    let mut spans: Vec<MaskSpan> = Vec::new();
    for &value in mask {
        match spans.last_mut() {
            Some(span) if span.value == value => span.length += 1,
            _ => spans.push(MaskSpan { value, length: 1 }),
        }
    }
    spans
}

/// Decodes spans back into a flat mask. Total coverage is preserved exactly; an empty
/// span list decodes to an empty mask.
pub fn spans_to_mask(spans: &[MaskSpan]) -> Vec<u8> {
    let mut mask = Vec::new();
    for span in spans {
        let count = span.length as usize;
        mask.reserve(count);
        for _ in 0..count {
            mask.push(span.value);
        }
    }
    mask
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ocr_blocks_validate_and_order_by_reading_flow() {
        let block = OcrTextBlock::new("Hello", (10, 20, 100, 30), 0.95).expect("valid block");
        assert_eq!(block.text, "Hello");
        assert_eq!(block.region, (10, 20, 100, 30));

        // Validation rejections
        assert!(OcrTextBlock::new("   ", (0, 0, 1, 1), 0.9).is_err());
        assert!(OcrTextBlock::new("text", (0, 0, 1, 1), 1.5).is_err());
        assert!(OcrTextBlock::new("text", (0, 0, 0, 5), 0.9).is_err());

        // Reading order sorts by y then x regardless of input order
        let mut blocks = vec![
            OcrTextBlock::new("right", (200, 50, 50, 20), 1.0).unwrap(),
            OcrTextBlock::new("left", (10, 50, 50, 20), 1.0).unwrap(),
            OcrTextBlock::new("above", (0, 10, 50, 20), 1.0).unwrap(),
        ];
        blocks.sort_by(OcrTextBlock::reading_order);
        assert_eq!(
            blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>(),
            vec!["above", "left", "right"]
        );
    }

    #[test]
    fn mask_span_codec_round_trips() {
        let mask = vec![0, 0, 255, 255, 255, 0, 128];
        let spans = mask_to_spans(&mask);
        assert_eq!(
            spans,
            vec![
                MaskSpan {
                    value: 0,
                    length: 2
                },
                MaskSpan {
                    value: 255,
                    length: 3
                },
                MaskSpan {
                    value: 0,
                    length: 1
                },
                MaskSpan {
                    value: 128,
                    length: 1
                },
            ]
        );
        assert_eq!(spans_to_mask(&spans), mask);

        // Empty inputs stay empty both ways
        assert!(mask_to_spans(&[]).is_empty());
        assert!(spans_to_mask(&[]).is_empty());

        // Uniform masks compress to one span
        let flat = vec![7u8; 1000];
        assert_eq!(
            mask_to_spans(&flat),
            vec![MaskSpan {
                value: 7,
                length: 1000
            }]
        );
    }
}
