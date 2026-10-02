//! Write worksheet models to supported XLSX workbook parts.

use std::collections::BTreeSet;

use loom_package::zip::PackageArchive;

use crate::style::{CellAlignment, CellStyle};
use crate::{CellRef, ChartKind, Sheet, SheetChart, SheetObject, SheetObjectKind};

use super::cell_refs::{chart_sheet_reference, column_letters};
use super::styles::{render_styles_xml, rgb_for_fill, XfKey};
use super::worksheet_xml::{render_worksheet, SharedStrings, WorksheetInput};
use super::xml::{xml_escape_attr, xml_escape_text};
use super::{CHART_NS, DRAWINGML_NS, DRAWING_NS, EMU_PER_PIXEL, MAIN_NS, PACKAGE_REL_NS, REL_NS};

/// Export every worksheet with formulas, cell formatting, and supported
/// drawing objects represented in OOXML parts. The package is written the way
/// Microsoft Excel writes it (typed cached values, array formulas, sizes,
/// frozen panes, the reserved style entries) because Excel silently repairs,
/// or refuses, packages that break its rules.
pub fn export_xlsx_sheets(sheets: &[Sheet]) -> Result<Vec<u8>, String> {
    if sheets.is_empty() {
        return Err("xlsx export needs at least one sheet".to_string());
    }
    let evaluated = crate::workbook::evaluate_workbook(sheets);
    let names = crate::unique_xlsx_sheet_names(
        &sheets
            .iter()
            .map(|sheet| crate::XlsxSheetData {
                name: sheet.name.clone(),
                grid: Vec::new(),
            })
            .collect::<Vec<_>>(),
    )?;
    let exported_sheet_names = names.names;

    let mut style_keys = vec![XfKey {
        style: CellStyle::default(),
        alignment: CellAlignment::General,
    }];
    for sheet in sheets {
        let mut coordinates = BTreeSet::new();
        coordinates.extend(sheet.cells.keys().copied());
        coordinates.extend(sheet.styles.keys().copied());
        coordinates.extend(sheet.alignments.keys().copied());
        for cell in coordinates {
            let key = XfKey::from_sheet(sheet, cell);
            if !style_keys.contains(&key) {
                style_keys.push(key);
            }
        }
    }
    let style_ids = style_keys
        .iter()
        .enumerate()
        .map(|(id, key)| (*key, id))
        .collect::<Vec<_>>();
    let mut archive = PackageArchive::new();
    let mut add = |path: &str, bytes: Vec<u8>| {
        archive
            .add(path, bytes)
            .map_err(|e| format!("xlsx export failed: {e}"))
    };
    let mut strings = SharedStrings::default();
    let mut image_extensions = BTreeSet::new();
    let mut sheet_parts: Vec<(String, String)> = Vec::new();

    for (index, sheet) in sheets.iter().enumerate() {
        let has_drawing = sheet.chart.is_some() || !sheet.objects.is_empty();
        let worksheet = render_worksheet(
            &WorksheetInput {
                sheet,
                values: &evaluated[index],
                style_ids: &style_ids,
                sheet_names: &names.formula_mapping,
                selected: index == 0,
                has_drawing,
            },
            &mut strings,
        );
        sheet_parts.push((format!("xl/worksheets/sheet{}.xml", index + 1), worksheet));
        if has_drawing {
            let number = index + 1;
            let (drawing_xml, drawing_rels, chart_parts, image_parts, extensions) =
                render_drawing_parts(sheet, number, &exported_sheet_names[index])?;
            image_extensions.extend(extensions);
            add(
                &format!("xl/drawings/drawing{number}.xml"),
                drawing_xml.into_bytes(),
            )?;
            add(
                &format!("xl/drawings/_rels/drawing{number}.xml.rels"),
                drawing_rels.into_bytes(),
            )?;
            for (path, xml) in chart_parts {
                add(&path, xml.into_bytes())?;
            }
            for (path, bytes) in image_parts {
                add(&path, bytes)?;
            }
            add(
                &format!("xl/worksheets/_rels/sheet{number}.xml.rels"),
                worksheet_drawing_rels(number).into_bytes(),
            )?;
        }
    }
    for (path, xml) in sheet_parts {
        add(&path, xml.into_bytes())?;
    }
    add(
        "[Content_Types].xml",
        render_content_types(sheets, &image_extensions).into_bytes(),
    )?;
    add("_rels/.rels", ROOT_RELATIONSHIPS.as_bytes().to_vec())?;
    add(
        "xl/workbook.xml",
        render_workbook(&exported_sheet_names).into_bytes(),
    )?;
    add(
        "xl/_rels/workbook.xml.rels",
        render_workbook_relationships(sheets.len()).into_bytes(),
    )?;
    add("xl/sharedStrings.xml", strings.render().into_bytes())?;
    add("xl/styles.xml", render_styles_xml(&style_keys).into_bytes())?;
    archive
        .to_bytes()
        .map_err(|e| format!("xlsx export failed: {e}"))
}

const ROOT_RELATIONSHIPS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>";

fn render_workbook(sheet_names: &[String]) -> String {
    let sheets = sheet_names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            format!(
                "<sheet name=\"{}\" sheetId=\"{1}\" r:id=\"rId{1}\"/>",
                xml_escape_attr(name),
                index + 1
            )
        })
        .collect::<String>();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><workbook xmlns=\"{MAIN_NS}\" xmlns:r=\"{REL_NS}\"><workbookPr/><bookViews><workbookView xWindow=\"0\" yWindow=\"0\" windowWidth=\"28800\" windowHeight=\"12300\" activeTab=\"0\"/></bookViews><sheets>{sheets}</sheets><calcPr calcId=\"191029\" fullCalcOnLoad=\"1\"/></workbook>"
    )
}

fn render_workbook_relationships(sheet_count: usize) -> String {
    let mut items = (1..=sheet_count)
        .map(|part| {
            format!(
                "<Relationship Id=\"rId{part}\" Type=\"{REL_NS}/worksheet\" Target=\"worksheets/sheet{part}.xml\"/>"
            )
        })
        .collect::<String>();
    items.push_str(&format!(
        "<Relationship Id=\"rId{}\" Type=\"{REL_NS}/sharedStrings\" Target=\"sharedStrings.xml\"/><Relationship Id=\"rId{}\" Type=\"{REL_NS}/styles\" Target=\"styles.xml\"/>",
        sheet_count + 1,
        sheet_count + 2
    ));
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"{PACKAGE_REL_NS}\">{items}</Relationships>"
    )
}

type DrawingParts = (
    String,
    String,
    Vec<(String, String)>,
    Vec<(String, Vec<u8>)>,
    BTreeSet<String>,
);

fn render_drawing_parts(
    sheet: &Sheet,
    sheet_number: usize,
    exported_sheet_name: &str,
) -> Result<DrawingParts, String> {
    let mut drawing = String::new();
    let mut drawing_rels = String::new();
    let mut chart_parts = Vec::new();
    let mut image_parts = Vec::new();
    let mut image_extensions = BTreeSet::new();
    let mut rel_id = 1usize;
    let mut object_id = 1usize;

    if let Some(chart) = &sheet.chart {
        let chart_rel = format!("rId{rel_id}");
        rel_id += 1;
        drawing_rels.push_str(&format!(
            "<Relationship Id=\"{chart_rel}\" Type=\"{CHART_REL_TYPE}\" Target=\"../charts/chart{sheet_number}.xml\"/>"
        ));
        chart_parts.push((
            format!("xl/charts/chart{sheet_number}.xml"),
            render_chart_xml(sheet, chart, exported_sheet_name),
        ));
        // The model keeps no chart position. Start two columns past the data
        // (column D at the smallest) so the chart never covers cells.
        let chart_col = sheet
            .used_range()
            .map_or(3, |(_, _, max_col, _)| (max_col + 2).max(3));
        drawing.push_str(&one_cell_anchor(
            CellRef {
                row: 0,
                col: chart_col,
            },
            560,
            320,
            &format!(
                "<xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{object_id}\" name=\"Chart {object_id}\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></xdr:xfrm><a:graphic><a:graphicData uri=\"{CHART_GRAPHIC_URI}\"><c:chart r:id=\"{chart_rel}\"/></a:graphicData></a:graphic></xdr:graphicFrame>"
            ),
        ));
        object_id += 1;
    }

    for (object_index, object) in sheet.objects.iter().enumerate() {
        match object.kind {
            SheetObjectKind::Shape => {
                drawing.push_str(&one_cell_anchor(
                    object.anchor,
                    object.width,
                    object.height,
                    &format!(
                        "<xdr:sp><xdr:nvSpPr><xdr:cNvPr id=\"{object_id}\" name=\"Shape {object_id}\" descr=\"{}\"/><xdr:cNvSpPr txBox=\"0\"/></xdr:nvSpPr><xdr:spPr><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill><a:ln w=\"9525\"><a:solidFill><a:srgbClr val=\"9CA3AF\"/></a:solidFill></a:ln></xdr:spPr><xdr:txBody><a:bodyPr vertOverflow=\"clip\" wrap=\"square\" rtlCol=\"0\" anchor=\"ctr\"/><a:lstStyle/><a:p><a:pPr algn=\"ctr\"/><a:r><a:rPr lang=\"en-US\" sz=\"1100\"><a:solidFill><a:srgbClr val=\"1F2937\"/></a:solidFill></a:rPr><a:t>{}</a:t></a:r></a:p></xdr:txBody></xdr:sp>",
                        xml_escape_attr(&object.label),
                        rgb_for_fill(object.fill),
                        xml_escape_text(&object.label),
                    ),
                ));
                object_id += 1;
            }
            SheetObjectKind::Image => {
                let payload = object
                    .embedded
                    .clone()
                    .or_else(|| std::fs::read(&object.path).ok())
                    .ok_or_else(|| {
                        if object.path.trim().is_empty() {
                            "image object has no embedded payload or source path".to_string()
                        } else {
                            format!("unable to read image {}", object.path)
                        }
                    })?;
                let extension = image_extension_for_object(object);
                image_extensions.insert(extension.to_string());
                let filename = format!("sheet{sheet_number}-object{object_index}.{extension}");
                let image_rel = format!("rId{rel_id}");
                rel_id += 1;
                drawing_rels.push_str(&format!(
                    "<Relationship Id=\"{image_rel}\" Type=\"{IMAGE_REL_TYPE}\" Target=\"../media/{filename}\"/>"
                ));
                image_parts.push((format!("xl/media/{filename}"), payload));
                drawing.push_str(&one_cell_anchor(
                    object.anchor,
                    object.width,
                    object.height,
                    &format!(
                        "<xdr:pic><xdr:nvPicPr><xdr:cNvPr id=\"{object_id}\" name=\"Image {object_id}\" descr=\"{}\"/><xdr:cNvPicPr/></xdr:nvPicPr><xdr:blipFill><a:blip r:embed=\"{image_rel}\"/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic>",
                        xml_escape_attr(&object.label),
                    ),
                ));
                object_id += 1;
            }
        }
    }
    Ok((
        format!(
            "<?xml version=\"1.0\"?><xdr:wsDr xmlns:xdr=\"{DRAWING_NS}\" xmlns:a=\"{DRAWINGML_NS}\" xmlns:c=\"{CHART_NS}\" xmlns:r=\"{REL_NS}\">{drawing}</xdr:wsDr>"
        ),
        format!(
            "<?xml version=\"1.0\"?><Relationships xmlns=\"{PACKAGE_REL_NS}\">{drawing_rels}</Relationships>"
        ),
        chart_parts,
        image_parts,
        image_extensions,
    ))
}

fn one_cell_anchor(cell: CellRef, width: u32, height: u32, content: &str) -> String {
    let cx = (width.max(1) as f32 * EMU_PER_PIXEL).round() as u64;
    let cy = (height.max(1) as f32 * EMU_PER_PIXEL).round() as u64;
    format!(
        "<xdr:oneCellAnchor><xdr:from><xdr:col>{}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:ext cx=\"{cx}\" cy=\"{cy}\"/>{content}<xdr:clientData/></xdr:oneCellAnchor>",
        cell.col, cell.row
    )
}

fn render_chart_xml(sheet: &Sheet, chart: &SheetChart, exported_sheet_name: &str) -> String {
    let start_row = chart.start_row + 1;
    let end_row = chart.end_row.map(|row| row + 1).unwrap_or_else(|| {
        sheet
            .used_range()
            .map(|(_, _, _, max_row)| max_row + 1)
            .unwrap_or(2)
            .max(2)
    });
    let name = chart_sheet_reference(exported_sheet_name);
    let cat_column = column_letters(chart.cat_col as usize);
    let val_column = column_letters(chart.val_col as usize);
    // Spreadsheet quoting and XML escaping are separate operations. The
    // sheet name is quoted for the formula grammar above; escape the complete
    // generated formula before putting it in element text.
    let cat_range = xml_escape_text(&format!(
        "{name}!${cat_column}${start_row}:${cat_column}${end_row}"
    ));
    let val_range = xml_escape_text(&format!(
        "{name}!${val_column}${start_row}:${val_column}${end_row}"
    ));
    let title = xml_escape_text(&chart.title);
    let series = match chart.kind {
        ChartKind::Scatter => format!(
            "<c:scatterChart><c:scatterStyle val=\"lineMarker\"/><c:varyColors val=\"0\"/><c:ser><c:idx val=\"0\"/><c:order val=\"0\"/><c:xVal><c:numRef><c:f>{cat_range}</c:f></c:numRef></c:xVal><c:yVal><c:numRef><c:f>{val_range}</c:f></c:numRef></c:yVal></c:ser><c:axId val=\"-2128941756\"/><c:axId val=\"-2128941755\"/></c:scatterChart>"
        ),
        ChartKind::Pie => format!(
            "<c:pieChart><c:varyColors val=\"1\"/><c:ser><c:idx val=\"0\"/><c:order val=\"0\"/><c:cat><c:strRef><c:f>{cat_range}</c:f></c:strRef></c:cat><c:val><c:numRef><c:f>{val_range}</c:f></c:numRef></c:val></c:ser></c:pieChart>"
        ),
        ChartKind::Line => format!(
            "<c:lineChart><c:grouping val=\"standard\"/><c:varyColors val=\"0\"/><c:ser><c:idx val=\"0\"/><c:order val=\"0\"/><c:cat><c:strRef><c:f>{cat_range}</c:f></c:strRef></c:cat><c:val><c:numRef><c:f>{val_range}</c:f></c:numRef></c:val></c:ser><c:axId val=\"-2128941756\"/><c:axId val=\"-2128941755\"/></c:lineChart>"
        ),
        ChartKind::Bar => format!(
            "<c:barChart><c:barDir val=\"col\"/><c:grouping val=\"clustered\"/><c:varyColors val=\"0\"/><c:ser><c:idx val=\"0\"/><c:order val=\"0\"/><c:cat><c:strRef><c:f>{cat_range}</c:f></c:strRef></c:cat><c:val><c:numRef><c:f>{val_range}</c:f></c:numRef></c:val></c:ser><c:axId val=\"-2128941756\"/><c:axId val=\"-2128941755\"/></c:barChart>"
        ),
    };
    let axes = match chart.kind {
        ChartKind::Bar | ChartKind::Line => "<c:catAx><c:axId val=\"-2128941756\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/><c:crossAx val=\"-2128941755\"/><c:crosses val=\"autoZero\"/><c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/></c:catAx><c:valAx><c:axId val=\"-2128941755\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"l\"/><c:crossAx val=\"-2128941756\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"midCat\"/></c:valAx>",
        ChartKind::Scatter => "<c:valAx><c:axId val=\"-2128941756\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/><c:crossAx val=\"-2128941755\"/><c:crosses val=\"autoZero\"/></c:valAx><c:valAx><c:axId val=\"-2128941755\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"l\"/><c:crossAx val=\"-2128941756\"/><c:crosses val=\"autoZero\"/></c:valAx>",
        ChartKind::Pie => "",
    };
    format!(
        "<?xml version=\"1.0\"?><c:chartSpace xmlns:c=\"{CHART_NS}\" xmlns:a=\"{DRAWINGML_NS}\"><c:chart><c:title><c:tx><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\"/><a:t>{title}</a:t></a:r></a:p></c:rich></c:tx><c:layout/></c:title><c:plotArea><c:layout/>{series}{axes}</c:plotArea><c:plotVisOnly val=\"1\"/></c:chart></c:chartSpace>"
    )
}

fn worksheet_drawing_rels(drawing_number: usize) -> String {
    format!(
        "<?xml version=\"1.0\"?><Relationships xmlns=\"{PACKAGE_REL_NS}\"><Relationship Id=\"rId1\" Type=\"{DRAWING_REL_TYPE}\" Target=\"../drawings/drawing{drawing_number}.xml\"/></Relationships>"
    )
}

fn render_content_types(sheets: &[Sheet], image_extensions: &BTreeSet<String>) -> String {
    let mut additions = String::new();
    for (index, sheet) in sheets.iter().enumerate() {
        additions.push_str(&format!(
            "<Override PartName=\"/xl/worksheets/sheet{}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>",
            index + 1
        ));
        if sheet.chart.is_some() || !sheet.objects.is_empty() {
            additions.push_str(&format!(
                "<Override PartName=\"/xl/drawings/drawing{}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawing+xml\"/>",
                index + 1
            ));
        }
        if sheet.chart.is_some() {
            additions.push_str(&format!(
                "<Override PartName=\"/xl/charts/chart{}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawingml.chart+xml\"/>",
                index + 1
            ));
        }
    }
    let mut defaults = String::new();
    for extension in image_extensions {
        let mime = match extension.as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "svg" => "image/svg+xml",
            _ => "image/png",
        };
        defaults.push_str(&format!(
            "<Default Extension=\"{extension}\" ContentType=\"{mime}\"/>"
        ));
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/>{defaults}<Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/><Override PartName=\"/xl/sharedStrings.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml\"/>{additions}</Types>"
    )
}

const DRAWING_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing";
const CHART_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart";
const IMAGE_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
const CHART_GRAPHIC_URI: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";

fn image_extension_for_object(object: &SheetObject) -> &'static str {
    let source = if object.path.trim().is_empty() {
        object.asset.as_deref().unwrap_or("image.png")
    } else {
        &object.path
    };
    match source
        .rsplit('.')
        .next()
        .unwrap_or("png")
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "jpg",
        "gif" => "gif",
        "webp" => "webp",
        "svg" | "svgz" => "svg",
        _ => "png",
    }
}
