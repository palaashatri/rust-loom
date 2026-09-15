use loom_sheets_core::{Sheet, CellRef, SheetChart, ChartKind, SheetObject, sheet_to_json, sheet_from_json, workbook_to_json, workbook_from_json, extract_xlsx_sheets, export_xlsx_sheets};
use loom_package::zip::PackageArchive;
fn main() {
    for raw in ["one\ttwo", "one\rtwo", r"C:\new\notes", "before\"}after"] {
        let mut sheet = Sheet::new("Data");
        sheet.set_str("A1", raw);
        let out = sheet_from_json(&sheet_to_json(&sheet)).unwrap();
        println!("JSON input={raw:?} output={:?}", out.raw(CellRef {row:0,col:0}));
    }
    let sheet = Sheet::new("My \"Sheet\"");
    let back = sheet_from_json(&sheet_to_json(&sheet)).unwrap();
    println!("JSON name={:?} output={:?}", sheet.name, back.name);
    let mut archive = PackageArchive::new();
    archive.add("xl/worksheets/sheet1.xml", br#"<worksheet><sheetData><row r="1"><c r="A1"><v>10</v></c><c r="B1"><f t="shared" ref="B1:B2" si="0">A1*2</f><v>20</v></c></row><row r="2"><c r="A2"><v>15</v></c><c r="B2"><f t="shared" si="0"/><v>30</v></c></row></sheetData></worksheet>"#.to_vec()).unwrap();
    let mut shared = extract_xlsx_sheets(&archive.to_bytes().unwrap()).unwrap();
    println!("XLSX shared formula B1={:?} B2={:?}", shared[0].raw(CellRef {row:0,col:1}), shared[0].raw(CellRef {row:1,col:1}));
    shared[0].set_str("A2", "50");
    println!("XLSX after A2=50 B2={:?}", loom_sheets_core::workbook::evaluate_workbook(&shared)[0].get(&CellRef{row:1,col:1}));
    let mut sheet = Sheet::new("R&D");
    sheet.set_str("A1", "label"); sheet.set_str("B1", "value"); sheet.set_str("A2", "Q1"); sheet.set_str("B2", "100");
    sheet.chart = Some(SheetChart {kind:ChartKind::Bar,title:"Revenue".into(),cat_col:0,val_col:1});
    let bytes = export_xlsx_sheets(&[sheet]).unwrap();
    std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/chart.xlsx"), bytes).unwrap();
    let mut sheet = Sheet::new("Image");
    let mut object = SheetObject::image(CellRef{row:0,col:0},"/missing/original.png").unwrap();
    object.embedded = Some(vec![1,2,3]); object.asset=Some("content/assets/image-0-0.png".into());
    sheet.objects.push(object);
    let recovered = workbook_from_json(&workbook_to_json(&[sheet],0)).unwrap();
    println!("Recovery embedded={:?} asset={:?}", recovered.sheets[0].objects[0].embedded, recovered.sheets[0].objects[0].asset);
    println!("Recovered image export={:?}", export_xlsx_sheets(&recovered.sheets).map(|v|v.len()));
}
