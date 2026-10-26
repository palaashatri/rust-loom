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
}

struct RegisteredFace {
    face: Arc<LoadedFace>,
    pdf_id: PdfFontId,
}

impl FontRegistry {
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

pub(crate) fn draw_line(
    pdf: &mut PdfDocument,
    fonts: &mut FontRegistry,
    page: PageIndex,
    context: &str,
    text: &str,
    line_top_pdf: f32,
    text_origin_x: f32,
    shaped_line: &WriterShapedVisualLine,
) -> Result<(), WriterError> {
    draw_line_with_prefix(
        pdf,
        fonts,
        page,
        context,
        None,
        text,
        line_top_pdf,
        text_origin_x,
        shaped_line,
    )
}

pub(crate) fn draw_line_with_prefix<'a>(
    pdf: &mut PdfDocument,
    fonts: &mut FontRegistry,
    page: PageIndex,
    context: &str,
    prefix: Option<(&'a str, &'a WriterShapedVisualLine, f32)>,
    text: &'a str,
    line_top_pdf: f32,
    text_origin_x: f32,
    shaped_line: &'a WriterShapedVisualLine,
) -> Result<(), WriterError> {
    let mut positioned = Vec::new();
    if let Some((prefix_text, prefix_line, prefix_origin_x)) = prefix {
        let prefix_top = line_top_pdf - shaped_line.ascent_pt() + prefix_line.ascent_pt();
        append_positioned_runs(
            pdf,
            fonts,
            context,
            prefix_text,
            prefix_top,
            prefix_origin_x,
            prefix_line,
            &mut positioned,
        )?;
    }
    append_positioned_runs(
        pdf,
        fonts,
        context,
        text,
        line_top_pdf,
        text_origin_x,
        shaped_line,
        &mut positioned,
    )?;
    pdf.draw_styled_shaped_text_line(page, &positioned)
        .map_err(|error| pdf_error(format!("{context}: {error}")))
}

fn append_positioned_runs<'a>(
    pdf: &mut PdfDocument,
    fonts: &mut FontRegistry,
    context: &str,
    text: &'a str,
    line_top_pdf: f32,
    text_origin_x: f32,
    shaped_line: &'a WriterShapedVisualLine,
    positioned: &mut Vec<StyledPositionedShapedRun<'a>>,
) -> Result<(), WriterError> {
    let painted_runs = shaped_line.painted_runs();
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
        let Some(run_text) = text.get(run.source_range.clone()) else {
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
        let baseline = line_top_pdf - shaped_line.ascent_pt() + run.baseline_shift_pt;
        let foreground_rgba = if run.foreground_color_explicit {
            run.rgba
        } else {
            [0, 0, 0, 255]
        };
        let shaped = PositionedShapedRun {
            x: text_origin_x + run.x_pt,
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
