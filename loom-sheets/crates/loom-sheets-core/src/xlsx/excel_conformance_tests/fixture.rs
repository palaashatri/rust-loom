//! The rich workbook used to compare the exporter against real Excel.

use crate::style::{CellAlignment, CellStyle, FillColor};
use crate::{CellRef, ChartKind, NumberFormat, Sheet, SheetChart, SheetObject};

fn cell(a1: &str) -> CellRef {
    CellRef::parse(a1).expect("valid A1 reference")
}

fn style(sheet: &mut Sheet, a1: &str, edit: impl FnOnce(&mut CellStyle)) {
    let mut value = sheet.cell_style(cell(a1));
    edit(&mut value);
    sheet.set_cell_style(cell(a1), value);
}

/// A valid 24x24 RGB PNG, built by hand (stored deflate blocks) so the
/// fixture needs no image crate and Excel can really decode it.
pub(super) fn tiny_png() -> Vec<u8> {
    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &byte in bytes {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }
    fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc32(&body).to_be_bytes());
        out
    }
    const SIDE: usize = 24;
    let mut raw = Vec::new();
    for y in 0..SIDE {
        raw.push(0u8); // filter: none
        for x in 0..SIDE {
            raw.extend_from_slice(&[(x * 10) as u8, (y * 10) as u8, 160]);
        }
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in &raw {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    let mut zlib = vec![0x78, 0x01, 0x01];
    zlib.extend_from_slice(&(raw.len() as u16).to_le_bytes());
    zlib.extend_from_slice(&(!(raw.len() as u16)).to_le_bytes());
    zlib.extend_from_slice(&raw);
    zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut header = Vec::new();
    header.extend_from_slice(&(SIDE as u32).to_be_bytes());
    header.extend_from_slice(&(SIDE as u32).to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    png.extend(chunk(b"IHDR", &header));
    png.extend(chunk(b"IDAT", &zlib));
    png.extend(chunk(b"IEND", &[]));
    png
}

/// Four sheets covering values, text edge cases, formulas, styles, sizes,
/// freeze panes, a chart, a shape, an image, dynamic arrays and modern
/// functions.
pub(crate) fn rich_interop_workbook() -> Vec<Sheet> {
    vec![sales(), summary(), modern(), arrays()]
}

fn sales() -> Sheet {
    let mut s = Sheet::new("Sales");
    for (a1, raw) in [
        ("A1", "Item"),
        ("B1", "Qty"),
        ("C1", "Price"),
        ("D1", "Total"),
        ("E1", "Notes"),
        ("F1", "Misc"),
        ("A2", "Widget"),
        ("B2", "3"),
        ("C2", "1.25"),
        ("D2", "=B2*C2"),
        ("E2", "He said \"hi\", café – 日本語 <&> 'q'"),
        ("F2", "0.256"),
        ("A3", "Gadget"),
        ("B3", "10"),
        ("C3", "0.5"),
        ("D3", "=B3*C3"),
        ("E3", "line one\nline two"),
        ("F3", "0.5"),
        ("A4", "Gizmo"),
        ("B4", "7"),
        ("C4", "2.75"),
        ("D4", "=B4*C4"),
        ("E4", "#1 seller"),
        ("F4", "1234.5678"),
        ("A5", "Total"),
        ("B5", "=SUM(B2:B4)"),
        ("C5", "=AVERAGE(C2:C4)"),
        ("D5", "=SUM(D2:D4)"),
        ("E5", "=IF(D5>20,\"big\",\"small\")"),
        ("F5", "123456789"),
        ("A6", "Lookup"),
        ("B6", "=VLOOKUP(\"Gadget\",A2:D4,4,FALSE)"),
        ("C6", "=INDEX(C2:C4,2)"),
        ("D6", "=MATCH(\"Gizmo\",A2:A4,0)"),
        ("E6", "=COUNTIF(A2:A4,\"G*\")"),
        ("F6", "46296"),
        ("A7", "Stats"),
        ("B7", "=MAX(B2:B4)"),
        ("C7", "=MIN(C2:C4)"),
        ("D7", "=ROUND(D5/3,2)"),
        ("E7", "=SUMIF(B2:B4,\">5\",D2:D4)"),
        ("A8", "Absolute"),
        ("B8", "=$B$2*$C$2"),
        ("C8", "='Summary & Notes'!B2"),
        ("D8", "=Sales!D5-D2"),
        ("A9", "Flags"),
        ("B9", "TRUE"),
        ("C9", "FALSE"),
        ("D9", "=AND(B9,C9)"),
        ("E9", "=1/0"),
        ("A10", "Text"),
        ("B10", "=UPPER(A2)"),
        ("C10", "=LEFT(A3,3)"),
        ("D10", "=LEN(E2)"),
        ("E10", "=CONCATENATE(A2,\"-\",A3)"),
        ("A11", "Empty ref"),
        ("B11", "=Z99"),
        ("C11", "=NOT(B9)"),
        ("D11", "=OR(B9,C9)"),
        ("E11", "=MID(E2,4,5)"),
        ("A12", "Bad syntax"),
        ("B12", "=SUM("),
        ("C12", "=NOPE(1)"),
    ] {
        s.set_str(a1, raw);
    }
    // Header row: bold, filled, bordered, centered.
    for a1 in ["A1", "B1", "C1", "D1", "E1", "F1"] {
        style(&mut s, a1, |v| {
            v.bold = true;
            v.fill = FillColor::Gray;
            v.border = true;
        });
        s.set_cell_alignment(cell(a1), CellAlignment::Center);
    }
    for a1 in ["C2", "C3", "C4"] {
        style(&mut s, a1, |v| {
            v.number_format = NumberFormat::Currency;
            v.decimal_places = Some(2);
        });
    }
    for a1 in ["D2", "D3", "D4", "D5"] {
        style(&mut s, a1, |v| {
            v.number_format = NumberFormat::Currency;
            v.fill = FillColor::Green;
        });
    }
    style(&mut s, "F2", |v| {
        v.number_format = NumberFormat::Percentage;
        v.decimal_places = Some(1);
    });
    style(&mut s, "F3", |v| {
        v.number_format = NumberFormat::Percentage;
        v.decimal_places = Some(0);
    });
    style(&mut s, "F4", |v| {
        v.number_format = NumberFormat::Number;
        v.decimal_places = Some(3);
    });
    style(&mut s, "F5", |v| {
        v.number_format = NumberFormat::Scientific;
        v.decimal_places = Some(2);
    });
    style(&mut s, "F6", |v| v.number_format = NumberFormat::DateIso);
    style(&mut s, "E2", |v| v.italic = true);
    style(&mut s, "E3", |v| v.underline = true);
    style(&mut s, "A5", |v| {
        v.bold = true;
        v.italic = true;
        v.underline = true;
        v.font_size = Some(16);
        v.fill = FillColor::Yellow;
        v.border = true;
    });
    s.set_cell_alignment(cell("A2"), CellAlignment::Left);
    s.set_cell_alignment(cell("A3"), CellAlignment::Center);
    s.set_cell_alignment(cell("A4"), CellAlignment::Right);
    for (a1, fill) in [
        ("A6", FillColor::Red),
        ("A7", FillColor::Orange),
        ("A8", FillColor::Blue),
        ("A9", FillColor::Purple),
    ] {
        style(&mut s, a1, |v| v.fill = fill);
    }
    // Styled cells that hold no value: one in a row that exists (G1) and one
    // in a row with no other content (G20).
    style(&mut s, "G1", |v| v.fill = FillColor::Orange);
    style(&mut s, "G20", |v| {
        v.bold = true;
        v.fill = FillColor::Blue;
    });

    s.set_col_width(0, 120.0);
    s.set_col_width(1, 60.0);
    s.set_col_width(4, 240.0);
    s.set_row_height(0, 36.0);
    s.set_row_height(2, 48.0);
    s.freeze_rows = 1;
    s.freeze_cols = 1;

    s.chart = Some(SheetChart {
        kind: ChartKind::Bar,
        title: "Totals by item <USD>".to_string(),
        cat_col: 0,
        val_col: 3,
        start_row: 1,
        end_row: Some(3),
    });
    let mut shape = SheetObject::shape(cell("I22"), "Quarterly target");
    shape.fill = FillColor::Yellow;
    s.objects.push(shape);
    let mut image = SheetObject::image(cell("M22"), "hero.png").expect("image object");
    image.embedded = Some(tiny_png());
    image.width = 96;
    image.height = 96;
    s.objects.push(image);
    s
}

fn summary() -> Sheet {
    let mut s = Sheet::new("Summary & Notes");
    for (a1, raw) in [
        ("A1", "Metric"),
        ("B1", "Value"),
        ("A2", "Grand total"),
        ("B2", "=Sales!D5*2"),
        ("A3", "First price"),
        ("B3", "=Sales!$C$2+1"),
        ("A4", "Doubled"),
        ("B4", "='Summary & Notes'!B2*2"),
        ("A5", "Items"),
        ("B5", "=COUNTA(Sales!A2:A4)"),
        ("A6", "Median"),
        ("B6", "=MEDIAN(Sales!B2:B4)"),
        ("A7", "Count"),
        ("B7", "=COUNT(Sales!B2:D4)"),
        ("A8", "Unicode, \"quoted\" label"),
        ("B8", "=Sales!A2&\" & \"&Sales!A3"),
    ] {
        s.set_str(a1, raw);
    }
    style(&mut s, "A1", |v| v.bold = true);
    style(&mut s, "B1", |v| v.bold = true);
    s.set_col_width(0, 200.0);
    s
}

fn modern() -> Sheet {
    let mut s = Sheet::new("Modern");
    for (a1, raw) in [
        ("A1", "=TEXTJOIN(\", \",TRUE,Sales!A2:A4)"),
        ("A2", "=CONCAT(Sales!A2,\"-\",Sales!A3)"),
        ("A3", "=MINIFS(Sales!D2:D4,Sales!B2:B4,\">3\")"),
        ("A4", "=MAXIFS(Sales!D2:D4,Sales!B2:B4,\">3\")"),
        ("A5", "=IFERROR(1/0,\"n/a\")"),
        ("A6", "=PMT(0.05/12,36,10000)"),
        ("A7", "=COUNT(Sales!B2:B4)"),
        ("A8", "=ROUND(2.5,0)"),
        ("A9", "=POWER(2,10)"),
        ("A10", "=MOD(10,3)"),
        ("A11", "=SQRT(16)"),
        ("A12", "=ABS(-4)"),
        ("A13", "=MEDIAN(1,3,5)"),
        ("A14", "=FLOOR(7.3,0.5)"),
        ("A15", "=CEILING(7.3,0.5)"),
        ("A16", "=TODAY()"),
        ("A17", "=AVERAGEIF(Sales!B2:B4,\">3\",Sales!D2:D4)"),
        ("A18", "=FV(0.05,10,-100)"),
        ("A19", "=PV(0.05,10,-100)"),
        ("A20", "=HLOOKUP(\"Qty\",Sales!A1:D4,2,FALSE)"),
        ("A21", "=TRIM(\"  a  b \")"),
        ("A22", "=LOWER(\"ABC\")"),
        ("A23", "=RIGHT(\"abcdef\",2)"),
        ("A24", "=COUNTA(Sales!A1:E11)"),
    ] {
        s.set_str(a1, raw);
    }
    s
}

fn arrays() -> Sheet {
    let mut s = Sheet::new("Arrays");
    for (a1, raw) in [
        ("A1", "=D2+1"),
        ("B1", "=SEQUENCE(2,3)"),
        ("A5", "b"),
        ("A6", "a"),
        ("A7", "b"),
        ("A8", "c"),
        ("C5", "=UNIQUE(A5:A8)"),
        ("E5", "=SORT(A5:A8)"),
        ("G5", "=TRANSPOSE(A5:A8)"),
        ("A12", "=SUM(B1:D2)"),
    ] {
        s.set_str(a1, raw);
    }
    s
}
