//! Bridge Writer's authoritative shaped paint runs to the PDF glyph writer.

use std::sync::Arc;

use loom_fonts::{Decoration, LoadedFace};
use loom_pdf::{
    PageIndex, PdfDocument, PdfFontId, PositionedRunDecoration, PositionedRunPaint,
    PositionedShapedRun, StyledPositionedShapedRun,
};

use crate::{WriterError, WriterPaintedRun, WriterShapedVisualLine};

#[derive(Default)]
pub(crate) struct FontRegistry {
    faces: Vec<RegisteredFace>,
    appearance: crate::PageAppearance,
}

struct RegisteredFace {
    face: Arc<LoadedFace>,
    pdf_id: PdfFontId,
}

impl FontRegistry {
    pub(crate) fn for_appearance(appearance: crate::PageAppearance) -> Self {
        Self {
            appearance,
            ..Self::default()
        }
    }

    fn id_for(
        &mut self,
        pdf: &mut PdfDocument,
        face: &Arc<LoadedFace>,
    ) -> Result<PdfFontId, String> {
        if let Some(existing) = self.faces.iter().find(|entry| {
            entry.face.collection_index() == face.collection_index()
                && entry.face.bytes() == face.bytes()
        }) {
            return Ok(existing.pdf_id);
        }

        let pdf_id = pdf.register_font_face(face.bytes(), face.collection_index())?;
        self.faces.push(RegisteredFace {
            face: Arc::clone(face),
            pdf_id,
        });
        Ok(pdf_id)
    }
}
pub(crate) struct PdfLine<'a> {
    pub(crate) text: &'a str,
    pub(crate) line_top_pdf: f32,
    pub(crate) text_origin_x: f32,
    pub(crate) shaped_line: &'a WriterShapedVisualLine,
}

pub(crate) struct PdfPrefix<'a> {
    pub(crate) text: &'a str,
    pub(crate) origin_x: f32,
    pub(crate) shaped_line: &'a WriterShapedVisualLine,
}

/// Add a PDF-only fallback for spans without a source foreground color.
pub(crate) fn with_automatic_ink(block: &crate::RichBlock) -> crate::RichBlock {
    let mut block = block.clone();
    let end = block.text.len_bytes();
    if end > 0 {
        // CharacterStyle defaults to explicit black; PDF auto ink is appearance-aware.
        block.runs.push(loom_text::StyleRun {
            start: 0,
            end,
            style: loom_text::CharacterStyle {
                color: None,
                ..Default::default()
            },
        });
    }
    block
}

pub(crate) fn draw_line(
    pdf: &mut PdfDocument,
    fonts: &mut FontRegistry,
    page: PageIndex,
    context: &str,
    line: PdfLine<'_>,
) -> Result<(), WriterError> {
    draw_line_with_prefix(pdf, fonts, page, context, None, line)
}

pub(crate) fn draw_line_with_prefix(
    pdf: &mut PdfDocument,
    fonts: &mut FontRegistry,
    page: PageIndex,
    context: &str,
    prefix: Option<PdfPrefix<'_>>,
    line: PdfLine<'_>,
) -> Result<(), WriterError> {
    let mut positioned = Vec::new();
    if let Some(prefix) = prefix {
        let prefix_top =
            line.line_top_pdf - line.shaped_line.ascent_pt() + prefix.shaped_line.ascent_pt();
        let prefix_line = PdfLine {
            text: prefix.text,
            line_top_pdf: prefix_top,
            text_origin_x: prefix.origin_x,
            shaped_line: prefix.shaped_line,
        };
        append_positioned_runs(pdf, fonts, context, &prefix_line, &mut positioned)?;
    }
    append_positioned_runs(pdf, fonts, context, &line, &mut positioned)?;
    pdf.draw_styled_shaped_text_line(page, &positioned)
        .map_err(|error| pdf_error(format!("{context}: {error}")))
}

fn append_positioned_runs<'a>(
    pdf: &mut PdfDocument,
    fonts: &mut FontRegistry,
    context: &str,
    line: &PdfLine<'a>,
    positioned: &mut Vec<StyledPositionedShapedRun<'a>>,
) -> Result<(), WriterError> {
    let painted_runs = line.shaped_line.painted_runs();
    for (run_index, run) in painted_runs.iter().enumerate() {
        if !run.variations.is_empty() {
            return Err(pdf_error(format!(
                "{context} run {run_index} uses a variable font instance, which PDF export does not support"
            )));
        }
    }

    let mut source_runs: Vec<&WriterPaintedRun> = painted_runs.iter().collect();
    source_runs.sort_by_key(|run| run.source_range.start);

    for (run_index, run) in source_runs.into_iter().enumerate() {
        let Some(run_text) = line.text.get(run.source_range.clone()) else {
            return Err(pdf_error(format!(
                "{context} run {run_index} has an invalid source range"
            )));
        };
        let font = fonts.id_for(pdf, &run.face).map_err(|error| {
            pdf_error(format!(
                "{context} run {run_index} font face {}: {error}",
                run.face.collection_index()
            ))
        })?;
        let baseline = line.line_top_pdf - line.shaped_line.ascent_pt() + run.baseline_shift_pt;
        let foreground_rgba = if run.foreground_color_explicit {
            run.rgba
        } else {
            fonts.appearance.auto_ink_rgba()
        };
        let shaped = PositionedShapedRun {
            x: line.text_origin_x + run.x_pt,
            y: baseline,
            text: run_text,
            shaped: &run.shaped,
            font,
            size_pt: run.size_pt,
            fill_rgb: rgb_tuple(foreground_rgba),
        };
        let paint = PositionedRunPaint {
            foreground_rgba,
            background_rgba: run.background_rgba,
            ascent_pt: run.ascent_pt,
            descent_pt: run.descent_pt,
            synthetic_bold: run.synthetic_bold,
            synthetic_italic: run.synthetic_italic,
            underline: run.underline.then(|| {
                decoration(
                    run.underline_metric,
                    -run.size_pt * 0.12,
                    (run.size_pt * 0.06).max(0.25),
                )
            }),
            strikethrough: run.strikethrough.then(|| {
                decoration(
                    run.strikeout_metric,
                    run.size_pt * 0.30,
                    (run.size_pt * 0.06).max(0.25),
                )
            }),
        };
        positioned.push(StyledPositionedShapedRun { run: shaped, paint });
    }
    Ok(())
}

fn decoration(
    metric: Option<Decoration>,
    fallback_offset: f32,
    fallback_thickness: f32,
) -> PositionedRunDecoration {
    let metric = metric.unwrap_or(Decoration {
        offset: fallback_offset,
        thickness: fallback_thickness,
    });
    PositionedRunDecoration {
        offset_pt: metric.offset,
        thickness_pt: metric.thickness.max(0.25),
    }
}

fn rgb_tuple(color: [u8; 4]) -> (f32, f32, f32) {
    (
        f32::from(color[0]) / 255.0,
        f32::from(color[1]) / 255.0,
        f32::from(color[2]) / 255.0,
    )
}

fn pdf_error(message: String) -> WriterError {
    WriterError::Invalid(format!("PDF export: {}", bounded(&message, 240)))
}

fn bounded(message: &str, max_chars: usize) -> String {
    let mut chars = message.chars();
    let value: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{value}…")
    } else {
        value
    }
}
