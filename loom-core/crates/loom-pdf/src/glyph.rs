//! Positioned glyph output for shaped Unicode text.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::sync::Arc;

use loom_fonts::Shaped;

use crate::fonts::{self, Font};
use crate::{clip01, fmt3, PageIndex, PdfDocument};

const MAX_TEXT_COORDINATE_PT: f64 = 1_000_000_000.0;

/// A validated font program that owns shared bytes instead of borrowing from
/// a process-long static allocation.
pub struct PdfFontProgram {
    font: Font,
}

impl PdfFontProgram {
    /// Parse one TrueType face from owned font bytes.
    ///
    /// The face index selects a face in a font collection. CFF outlines,
    /// variable instances, restricted/bitmap-only embedding, invalid indices,
    /// and collections that forbid subsetting are rejected.
    pub fn new(bytes: Arc<[u8]>, face_index: u32) -> Result<Self, String> {
        Ok(Self {
            font: Font::from_owned(bytes, face_index)?,
        })
    }
}

/// A font handle returned by [`PdfDocument::register_font_program`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PdfFontId(usize);

/// One already-shaped run in a logical text line.
///
/// Supply runs in logical source-text order. `x` is the run's visual left edge;
/// the glyph positions inside `shaped` remain in the shaper's visual order.
#[derive(Clone, Copy, Debug)]
pub struct PositionedShapedRun<'a> {
    /// Run's visual left edge in PDF user-space points.
    pub x: f32,
    /// Baseline in PDF user-space points.
    pub y: f32,
    /// Logical UTF-8 source text for this run.
    pub text: &'a str,
    /// Glyphs returned by the font shaper, in visual order.
    pub shaped: &'a Shaped,
    /// The font program that produced `shaped`.
    pub font: PdfFontId,
    /// Font size used to shape the run, in points.
    pub size_pt: f32,
    /// RGB fill color, with components clamped to 0..=1.
    pub fill_rgb: (f32, f32, f32),
}

/// A face-derived decoration in points from the run baseline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionedRunDecoration {
    pub offset_pt: f32,
    pub thickness_pt: f32,
}

/// Paint and fallback style for an already-shaped run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionedRunPaint {
    /// Straight-alpha RGBA foreground.
    pub foreground_rgba: [u8; 4],
    /// Optional straight-alpha run background.
    pub background_rgba: Option<[u8; 4]>,
    /// Run ascent from its baseline, in points.
    pub ascent_pt: f32,
    /// Run descent from its baseline, in points.
    pub descent_pt: f32,
    /// Raster-equivalent outline stroke for a face without a bold instance.
    pub synthetic_bold: bool,
    /// Raster-equivalent x shear for a face without an italic instance.
    pub synthetic_italic: bool,
    /// Face underline metric, or a caller-selected fallback.
    pub underline: Option<PositionedRunDecoration>,
    /// Face strikeout metric, or a caller-selected fallback.
    pub strikethrough: Option<PositionedRunDecoration>,
}

/// A shaped run with paint state kept alongside its glyph geometry.
#[derive(Clone, Copy, Debug)]
pub struct StyledPositionedShapedRun<'a> {
    pub run: PositionedShapedRun<'a>,
    pub paint: PositionedRunPaint,
}

#[derive(Clone)]
struct PlannedGlyph {
    source_index: usize,
    glyph_id: u16,
    x: f32,
    y: f32,
}

#[derive(Clone)]
struct PlannedCluster {
    text: Arc<str>,
    glyphs: Vec<PlannedGlyph>,
}

#[derive(Clone)]
struct PlannedRun {
    font: PdfFontId,
    size_pt: f32,
    fill_rgb: (f32, f32, f32),
    origin_x: f32,
    origin_y: f32,
    width: f32,
    paint: Option<PositionedRunPaint>,
    clusters: Vec<PlannedCluster>,
}

impl PdfDocument {
    /// Register a font program for shaped glyph runs. Registering identical
    /// programs reuses the existing document handle and embedded font.
    pub fn register_font_program(&mut self, program: PdfFontProgram) -> Result<PdfFontId, String> {
        self.register_font(program.font)
    }

    /// Register a face from borrowed catalog bytes. Bundled Inter faces reuse
    /// their process-static programs without first copying the source bytes;
    /// other faces are copied once into document-owned shared storage.
    pub fn register_font_face(
        &mut self,
        bytes: &[u8],
        face_index: u32,
    ) -> Result<PdfFontId, String> {
        fonts::validate_font_file_size(bytes.len())?;
        if let Some(index) =
            (0..4).find(|index| fonts::inter(*index).same_program_bytes(bytes, face_index))
        {
            return self.register_font(fonts::inter(index).clone());
        }
        let owned: Arc<[u8]> = Arc::from(bytes);
        self.register_font_program(PdfFontProgram::new(owned, face_index)?)
    }

    /// A handle for one of the bundled Inter faces without copying its static
    /// font bytes into an owned allocation.
    pub fn inter_font_id(&mut self, bold: bool, italic: bool) -> PdfFontId {
        self.register_font(fonts::inter(fonts::inter_index(bold, italic)).clone())
            .expect("bundled Inter program is valid")
    }

    fn register_font(&mut self, mut font: Font) -> Result<PdfFontId, String> {
        for index in 0..4 {
            let bundled = fonts::inter(index);
            if bundled.name == font.name {
                if !bundled.same_program(&font) {
                    return Err(format!(
                        "font name {:?} conflicts with a bundled Inter program",
                        font.name
                    ));
                }
                font = bundled.clone();
                break;
            }
        }

        if let Some((index, existing)) = self
            .font_programs
            .iter()
            .enumerate()
            .find(|(_, existing)| existing.name == font.name)
        {
            return if existing.same_program(&font) {
                Ok(PdfFontId(index))
            } else {
                Err(format!(
                    "font name {:?} was registered with different font bytes",
                    font.name
                ))
            };
        }
        if let Some(index) = self
            .font_programs
            .iter()
            .position(|existing| existing.same_program(&font))
        {
            return Ok(PdfFontId(index));
        }
        if let Some(existing) = self.faces.iter().find(|face| face.font_name() == font.name) {
            if !existing.same_program(&font) {
                return Err(format!(
                    "font name {:?} conflicts with a font already used by this document",
                    font.name
                ));
            }
        }

        let index = self.font_programs.len();
        self.font_programs.push(font);
        Ok(PdfFontId(index))
    }

    /// Draw already-shaped runs in logical text order. Each glyph is placed
    /// from the shaper's advances and offsets using an absolute text matrix;
    /// each cluster's `/ActualText` span is emitted once in logical order.
    ///
    /// The run's font must be the same static TrueType face that produced its
    /// `Shaped` data. Shape at `size_pt`. Invalid glyphs, cluster offsets,
    /// coordinates, or font handles fail before the page or subset remappers
    /// are changed.
    pub fn draw_shaped_text_line(
        &mut self,
        page: PageIndex,
        runs: &[PositionedShapedRun<'_>],
    ) -> Result<(), String> {
        if page.0 >= self.pages.len() {
            return Err(format!("page index {} is out of range", page.0));
        }
        if runs.is_empty() {
            return Ok(());
        }

        let planned = self.plan_runs(runs)?;
        self.preflight_glyph_capacity(&planned)?;
        self.write_shaped_line(page, &planned)
    }

    /// Draw already-shaped runs with straight-alpha color, optional
    /// backgrounds/decorations, and raster-equivalent synthetic styles.
    /// Runs are supplied in logical source-text order; glyph positions remain
    /// in the shaper's visual order.
    pub fn draw_styled_shaped_text_line(
        &mut self,
        page: PageIndex,
        runs: &[StyledPositionedShapedRun<'_>],
    ) -> Result<(), String> {
        if page.0 >= self.pages.len() {
            return Err(format!("page index {} is out of range", page.0));
        }
        if runs.is_empty() {
            return Ok(());
        }

        let mut planned = Vec::with_capacity(runs.len());
        for (run_index, styled) in runs.iter().enumerate() {
            let font = self
                .font_programs
                .get(styled.run.font.0)
                .ok_or_else(|| format!("run {run_index} has an unknown font handle"))?;
            validate_existing_face(&self.faces, font)?;
            validate_paint(run_index, &styled.run, styled.paint)?;
            let mut run = plan_run(run_index, &styled.run, font)?;
            run.paint = Some(styled.paint);
            planned.push(run);
        }
        self.preflight_glyph_capacity(&planned)?;
        self.write_styled_shaped_line(page, &planned)
    }

    fn plan_runs(&self, runs: &[PositionedShapedRun<'_>]) -> Result<Vec<PlannedRun>, String> {
        let mut planned = Vec::with_capacity(runs.len());
        for (run_index, run) in runs.iter().enumerate() {
            let font = self
                .font_programs
                .get(run.font.0)
                .ok_or_else(|| format!("run {run_index} has an unknown font handle"))?;
            validate_existing_face(&self.faces, font)?;
            planned.push(plan_run(run_index, run, font)?);
        }
        Ok(planned)
    }

    fn preflight_glyph_capacity(&self, planned: &[PlannedRun]) -> Result<(), String> {
        let mut glyphs_by_font: HashMap<usize, Vec<u16>> = HashMap::new();
        for run in planned {
            let glyphs = glyphs_by_font.entry(run.font.0).or_default();
            glyphs.extend(
                run.clusters
                    .iter()
                    .flat_map(|cluster| cluster.glyphs.iter().map(|glyph| glyph.glyph_id)),
            );
        }
        for (font_index, glyphs) in glyphs_by_font {
            let font = &self.font_programs[font_index];
            let slot = self
                .faces
                .iter()
                .position(|face| face.font_name() == font.name);
            let fits = if let Some(slot) = slot {
                self.faces[slot].can_encode_glyphs(&glyphs)
            } else {
                let mut unique = HashSet::from([0u16]);
                unique.extend(glyphs);
                unique.len() <= usize::from(u16::MAX)
            };
            if !fits {
                return Err(format!(
                    "font {:?} exceeds the PDF two-byte glyph limit",
                    font.name
                ));
            }
        }
        Ok(())
    }

    fn write_shaped_line(&mut self, page: PageIndex, planned: &[PlannedRun]) -> Result<(), String> {
        // All validation is complete. Slot creation and remapping below cannot
        // reject the already-checked programs or glyph sequence.
        let mut slots = Vec::with_capacity(planned.len());
        for run in planned {
            slots.push(self.font_slot(run.font)?);
        }

        let mut content = String::from("BT ");
        for (run, slot) in planned.iter().zip(slots) {
            let face_name = self.faces[slot].resource.clone();
            let (red, green, blue) = (
                clip01(run.fill_rgb.0),
                clip01(run.fill_rgb.1),
                clip01(run.fill_rgb.2),
            );
            let _ = write!(
                content,
                "{} {} {} rg /{face_name} {:.2} Tf ",
                fmt3(red),
                fmt3(green),
                fmt3(blue),
                run.size_pt,
            );
            for cluster in &run.clusters {
                let _ = write!(
                    content,
                    "/Span << /ActualText <FEFF{}> >> BDC ",
                    utf16_hex(&cluster.text)
                );
                for glyph in &cluster.glyphs {
                    let cid = self.faces[slot].encode_cluster_glyph(glyph.glyph_id, &cluster.text);
                    let _ = write!(
                        content,
                        "1 0 0 1 {} {} Tm <{cid:04X}> Tj ",
                        fmt3(glyph.x),
                        fmt3(glyph.y),
                    );
                }
                content.push_str("EMC ");
            }
        }
        content.push_str("ET");
        self.pages[page.0].ops.push(content);
        Ok(())
    }

    fn write_styled_shaped_line(
        &mut self,
        page: PageIndex,
        planned: &[PlannedRun],
    ) -> Result<(), String> {
        let mut slots = Vec::with_capacity(planned.len());
        for run in planned {
            slots.push(self.font_slot(run.font)?);
        }

        for run in planned {
            let paint = run.paint.expect("styled line plans retain paint");
            if let Some(background) = paint.background_rgba {
                let alpha = background[3];
                let state = self.record_alpha(page, alpha);
                let (red, green, blue) = rgb(background);
                let bottom = run.origin_y - paint.descent_pt;
                let height = paint.ascent_pt + paint.descent_pt;
                self.pages[page.0].ops.push(format!(
                    "q {state}{red} {green} {blue} rg {:.2} {:.2} {:.2} {:.2} re f Q",
                    run.origin_x, bottom, run.width, height,
                ));
            }
        }

        let mut content = String::from("q BT ");
        for (run, slot) in planned.iter().zip(slots) {
            let paint = run.paint.expect("styled line plans retain paint");
            let face_name = self.faces[slot].resource.clone();
            let alpha = paint.foreground_rgba[3];
            let state = self.record_alpha(page, alpha);
            let (red, green, blue) = rgb(paint.foreground_rgba);
            content.push_str(&state);
            content.push_str(&format!("{red} {green} {blue} rg "));
            if paint.synthetic_bold {
                let stroke_width = (run.size_pt / 24.0).max(0.25);
                let _ = write!(
                    content,
                    "{red} {green} {blue} RG {:.3} w 1 J 1 j 2 M 2 Tr ",
                    stroke_width,
                );
            } else {
                content.push_str("0 Tr ");
            }
            content.push_str(&format!("/{face_name} {:.2} Tf ", run.size_pt));
            for cluster in &run.clusters {
                let _ = write!(
                    content,
                    "/Span << /ActualText <FEFF{}> >> BDC ",
                    utf16_hex(&cluster.text)
                );
                for glyph in &cluster.glyphs {
                    let cid = self.faces[slot].encode_cluster_glyph(glyph.glyph_id, &cluster.text);
                    let matrix = if paint.synthetic_italic {
                        "1 0 0.208 1"
                    } else {
                        "1 0 0 1"
                    };
                    let _ = write!(
                        content,
                        "{matrix} {} {} Tm <{cid:04X}> Tj ",
                        fmt3(glyph.x),
                        fmt3(glyph.y),
                    );
                }
                content.push_str("EMC ");
            }
        }
        content.push_str("ET Q");
        self.pages[page.0].ops.push(content);

        for run in planned {
            let paint = run.paint.expect("styled line plans retain paint");
            for decoration in [paint.underline, paint.strikethrough].into_iter().flatten() {
                let alpha = paint.foreground_rgba[3];
                let state = self.record_alpha(page, alpha);
                let (red, green, blue) = rgb(paint.foreground_rgba);
                // `Decoration.offset_pt` identifies the decoration's top edge
                // from the baseline. PDF strokes are centred on their path,
                // so move the path down by half the thickness to match the
                // raster rectangle's top-to-bottom extent.
                let y = run.origin_y + decoration.offset_pt - decoration.thickness_pt / 2.0;
                self.pages[page.0].ops.push(format!(
                    "q {state}{red} {green} {blue} RG {:.3} w 0 J 0 j 2 M {:.2} {:.2} m {:.2} {:.2} l S Q",
                    decoration.thickness_pt,
                    run.origin_x,
                    y,
                    run.origin_x + run.width,
                    y,
                ));
            }
        }
        Ok(())
    }

    fn record_alpha(&mut self, page: PageIndex, alpha: u8) -> String {
        self.pages[page.0].alpha_states.insert(alpha);
        format!("/GS{alpha} gs ")
    }

    fn font_slot(&mut self, id: PdfFontId) -> Result<usize, String> {
        let font = self
            .font_programs
            .get(id.0)
            .ok_or_else(|| "unknown PDF font handle".to_string())?
            .clone();
        if let Some(index) = self
            .faces
            .iter()
            .position(|face| face.font_name() == font.name)
        {
            if self.faces[index].same_program(&font) {
                return Ok(index);
            }
            return Err(format!(
                "font name {:?} conflicts with a font already used by this document",
                font.name
            ));
        }
        if let Some(index) = (0..4).find(|index| fonts::inter(*index).name == font.name) {
            if fonts::inter(index).same_program(&font) {
                return Ok(self.inter_slot(index));
            }
        }
        let custom = self
            .faces
            .iter()
            .filter(|face| !fonts::is_bundled(face.font_name()))
            .count();
        self.faces.push(crate::embed::FaceSlot::new(
            format!("F{}", 5 + custom),
            font,
        ));
        Ok(self.faces.len() - 1)
    }
}

fn validate_existing_face(faces: &[crate::embed::FaceSlot], font: &Font) -> Result<(), String> {
    if let Some(existing) = faces.iter().find(|face| face.font_name() == font.name) {
        if !existing.same_program(font) {
            return Err(format!(
                "font name {:?} conflicts with a font already used by this document",
                font.name
            ));
        }
    }
    Ok(())
}

fn plan_run(
    run_index: usize,
    run: &PositionedShapedRun<'_>,
    font: &Font,
) -> Result<PlannedRun, String> {
    if !run.size_pt.is_finite() || !(0.01..=14_400.0).contains(&run.size_pt) {
        return Err(format!("run {run_index} has an invalid font size"));
    }
    let origin_x = finite_coordinate(f64::from(run.x), run_index, "origin x")?;
    let origin_y = finite_coordinate(f64::from(run.y), run_index, "origin y")?;
    if run.text.is_empty() && !run.shaped.glyphs.is_empty() {
        return Err(format!("run {run_index} has glyphs but no logical text"));
    }

    let mut groups: BTreeMap<usize, Vec<PlannedGlyph>> = BTreeMap::new();
    let mut pen_x = origin_x;
    for (source_index, glyph) in run.shaped.glyphs.iter().enumerate() {
        if glyph.cluster >= run.text.len() || !run.text.is_char_boundary(glyph.cluster) {
            return Err(format!(
                "run {run_index} glyph {source_index} has a non-UTF-8 cluster offset"
            ));
        }
        if glyph.glyph_id >= font.number_of_glyphs() {
            return Err(format!(
                "run {run_index} glyph {source_index} id {} is outside font {:?}",
                glyph.glyph_id, font.name
            ));
        }
        if !glyph.advance.is_finite() || !glyph.x_offset.is_finite() || !glyph.y_offset.is_finite()
        {
            return Err(format!(
                "run {run_index} glyph {source_index} has non-finite geometry"
            ));
        }
        let x = finite_coordinate(pen_x + f64::from(glyph.x_offset), run_index, "glyph x")?;
        let y = finite_coordinate(origin_y + f64::from(glyph.y_offset), run_index, "glyph y")?;
        groups.entry(glyph.cluster).or_default().push(PlannedGlyph {
            source_index,
            glyph_id: glyph.glyph_id,
            x: x as f32,
            y: y as f32,
        });
        pen_x = finite_coordinate(pen_x + f64::from(glyph.advance), run_index, "pen x")?;
    }

    let mut clusters = Vec::new();
    let starts: Vec<usize> = groups.keys().copied().collect();
    if !starts.is_empty() && starts.first() != Some(&0) {
        return Err(format!(
            "run {run_index} shaped clusters do not start at byte zero"
        ));
    }
    let mut covered = vec![false; run.shaped.glyphs.len()];
    for (group_index, start) in starts.iter().copied().enumerate() {
        let end = starts
            .get(group_index + 1)
            .copied()
            .unwrap_or(run.text.len());
        if end <= start || end > run.text.len() || !run.text.is_char_boundary(end) {
            return Err(format!(
                "run {run_index} has inconsistent cluster boundaries"
            ));
        }
        let text: Arc<str> = Arc::from(&run.text[start..end]);
        let glyphs = groups.remove(&start).unwrap_or_default();
        for glyph in &glyphs {
            let Some(seen) = covered.get_mut(glyph.source_index) else {
                return Err(format!("run {run_index} glyph coverage is out of range"));
            };
            if std::mem::replace(seen, true) {
                return Err(format!("run {run_index} glyph is covered more than once"));
            }
        }
        clusters.push(PlannedCluster { text, glyphs });
    }
    if covered.iter().any(|seen| !seen) {
        return Err(format!("run {run_index} has an uncovered shaped glyph"));
    }
    if groups.is_empty() && run.shaped.glyphs.is_empty() && !run.text.is_empty() {
        clusters.push(PlannedCluster {
            text: Arc::from(run.text),
            glyphs: Vec::new(),
        });
    }
    Ok(PlannedRun {
        font: run.font,
        size_pt: run.size_pt,
        fill_rgb: run.fill_rgb,
        origin_x: origin_x as f32,
        origin_y: origin_y as f32,
        width: run.shaped.width,
        paint: None,
        clusters,
    })
}

fn validate_paint(
    run_index: usize,
    run: &PositionedShapedRun<'_>,
    paint: PositionedRunPaint,
) -> Result<(), String> {
    let width = f64::from(run.shaped.width);
    if !width.is_finite() || width < 0.0 || width > MAX_TEXT_COORDINATE_PT {
        return Err(format!("run {run_index} has an invalid shaped width"));
    }
    if !paint.ascent_pt.is_finite()
        || !paint.descent_pt.is_finite()
        || paint.ascent_pt < 0.0
        || paint.descent_pt < 0.0
        || f64::from(paint.ascent_pt) > MAX_TEXT_COORDINATE_PT
        || f64::from(paint.descent_pt) > MAX_TEXT_COORDINATE_PT
    {
        return Err(format!("run {run_index} has invalid line metrics"));
    }
    finite_coordinate(
        f64::from(run.y) - f64::from(paint.descent_pt),
        run_index,
        "background bottom",
    )?;
    finite_coordinate(
        f64::from(run.y) + f64::from(paint.ascent_pt),
        run_index,
        "background top",
    )?;
    finite_coordinate(f64::from(run.x) + width, run_index, "run right edge")?;
    for (name, decoration) in [
        ("underline", paint.underline),
        ("strikethrough", paint.strikethrough),
    ] {
        if let Some(decoration) = decoration {
            if !decoration.offset_pt.is_finite()
                || !decoration.thickness_pt.is_finite()
                || decoration.thickness_pt <= 0.0
                || f64::from(decoration.thickness_pt) > MAX_TEXT_COORDINATE_PT
            {
                return Err(format!("run {run_index} has invalid {name} metrics"));
            }
            finite_coordinate(
                f64::from(run.y) + f64::from(decoration.offset_pt),
                run_index,
                "decoration y",
            )?;
        }
    }
    Ok(())
}

fn rgb(color: [u8; 4]) -> (String, String, String) {
    (
        fmt3(f32::from(color[0]) / 255.0),
        fmt3(f32::from(color[1]) / 255.0),
        fmt3(f32::from(color[2]) / 255.0),
    )
}

fn finite_coordinate(value: f64, run: usize, label: &str) -> Result<f64, String> {
    if value.is_finite() && value.abs() <= MAX_TEXT_COORDINATE_PT {
        Ok(value)
    } else {
        Err(format!("run {run} has an invalid {label} coordinate"))
    }
}

fn utf16_hex(text: &str) -> String {
    let mut hex = String::with_capacity(text.len() * 4);
    for unit in text.encode_utf16() {
        let _ = write!(hex, "{unit:04X}");
    }
    hex
}
