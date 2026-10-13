//! Integration tests for Wave 4: DOCX unsupported warning dialog,
//! clipboard chart embedding (canvas projection, DOCX and PDF export),
//! and Word 16.0 COM rich fixture round-trip.

use crate::actions_tests::test_state;
use crate::docx_actions::{open_docx_path, PENDING_DOCX_IMPORT};
use crate::{wire_writer_shared_callbacks, GuiState, WriterApp};
use loom_clipboard::{write_clipboard, ClipboardData};
use loom_objects::chart::{Chart, ChartSeries, ChartType};
use loom_package::zip::PackageArchive;
use loom_writer_core::{
    export_document_as_docx, export_pdf, import_docx, WriterDocument, CHART_BLOCK_KIND,
    TABLE_BLOCK_KIND,
};
use std::path::PathBuf;
use std::rc::Rc;

fn make_test_app() -> (WriterApp, Rc<GuiState>) {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let mut doc = WriterDocument::new("test-docx-app", "Original Document");
    doc.replace_paragraphs("Original baseline text");
    let (app, state) = test_state(doc, dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    (app, state)
}

fn sample_chart() -> Chart {
    Chart::from_data(
        "chart-quarterly",
        Some("Quarterly Performance".to_string()),
        ChartType::Bar,
        vec![
            "Q1".to_string(),
            "Q2".to_string(),
            "Q3".to_string(),
            "Q4".to_string(),
        ],
        vec![
            ChartSeries {
                name: "Revenue".to_string(),
                values: vec![100.0, 150.0, 200.0, 250.0],
                color: None,
            },
            ChartSeries {
                name: "Profit".to_string(),
                values: vec![20.0, 35.0, 50.0, 75.0],
                color: None,
            },
        ],
    )
}

/// Helper to create a minimal DOCX archive in memory with optional unsupported parts.
fn create_test_docx(include_smartart: bool, include_vba: bool) -> Vec<u8> {
    let mut archive = PackageArchive::new();
    let content_types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#;
    archive
        .add("[Content_Types].xml", content_types.as_bytes().to_vec())
        .unwrap();

    let doc_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>Imported document paragraph from DOCX</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    archive
        .add("word/document.xml", doc_xml.as_bytes().to_vec())
        .unwrap();

    if include_smartart {
        archive
            .add("word/diagrams/data1.xml", b"<smartArt/>".to_vec())
            .unwrap();
    }
    if include_vba {
        archive
            .add("word/vbaProject.bin", b"<macro/>".to_vec())
            .unwrap();
    }

    archive.to_bytes().expect("create test docx")
}

#[test]
fn test_open_docx_with_unsupported_warning_dialog_flow() {
    let (app, state) = make_test_app();
    assert_eq!(state.current.borrow().title, "Original Document");

    let docx_bytes = create_test_docx(true, false);
    let temp_dir =
        std::env::temp_dir().join(format!("loom-writer-docx-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();
    let docx_path = temp_dir.join("smartart_sample.docx");
    std::fs::write(&docx_path, &docx_bytes).unwrap();

    // 1. Open DOCX path: unsupported SmartArt is detected
    let replaced = open_docx_path(&app, &state, docx_path.clone(), &None).expect("open docx path");
    assert!(!replaced, "Must not replace document when warnings exist");
    assert!(
        app.get_docx_import_warning_open(),
        "Warning dialog must be open"
    );
    let warning_msg = app.get_docx_import_warning_message();
    assert!(
        warning_msg.contains("SmartArt"),
        "Warning message must list SmartArt: {warning_msg}"
    );

    // Document must remain untouched while warning is open
    assert_eq!(state.current.borrow().title, "Original Document");
    assert_eq!(
        state.current.borrow().blocks[0].text.as_str(),
        "Original baseline text"
    );

    // 2. Invoke Cancel: dialog closes, document and recovery remain untouched
    app.invoke_docx_import_cancel();
    assert!(
        !app.get_docx_import_warning_open(),
        "Warning dialog must close on cancel"
    );
    PENDING_DOCX_IMPORT.with(|p| assert!(p.borrow().is_none(), "Pending import must be cleared"));
    assert_eq!(state.current.borrow().title, "Original Document");
    assert_eq!(
        state.current.borrow().blocks[0].text.as_str(),
        "Original baseline text"
    );

    // 3. Open DOCX path again and invoke Continue
    let _ = open_docx_path(&app, &state, docx_path.clone(), &None).expect("open docx path again");
    assert!(app.get_docx_import_warning_open());
    app.invoke_docx_import_continue();
    assert!(
        !app.get_docx_import_warning_open(),
        "Warning dialog must close on continue"
    );

    // Document is now replaced with the imported DOCX!
    let current = state.current.borrow();
    assert!(
        current.blocks[0]
            .text
            .as_str()
            .contains("Imported document paragraph from DOCX"),
        "Document must contain imported paragraph: {}",
        current.blocks[0].text.as_str()
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_insert_chart_renders_and_exports_docx_and_pdf() {
    let (app, state) = make_test_app();
    let chart = sample_chart();
    let clip = ClipboardData::from_chart(&chart);
    write_clipboard(&clip).expect("write chart to clipboard");

    // Invoke insert chart command through app callback
    app.invoke_insert_chart();

    // Verify chart block is in current document
    let current = state.current.borrow();
    let chart_block = current
        .blocks
        .iter()
        .find(|b| b.kind == CHART_BLOCK_KIND)
        .expect("Chart block must be inserted");
    let parsed_chart = Chart::from_json(chart_block.text.as_str()).expect("Chart json must parse");
    assert_eq!(parsed_chart.title.as_deref(), Some("Quarterly Performance"));
    assert_eq!(parsed_chart.categories.len(), 4);
    assert_eq!(parsed_chart.series.len(), 2);
    assert_eq!(parsed_chart.series[0].name, "Revenue");

    // Verify DOCX export packages the chart as DrawingML part
    let docx_bytes = export_document_as_docx(&current).expect("export docx with chart");
    let archive = PackageArchive::from_bytes(&docx_bytes).expect("parse docx archive");
    assert!(
        archive.get("word/charts/chart1.xml").is_some(),
        "DOCX export must contain word/charts/chart1.xml"
    );
    let doc_xml = String::from_utf8_lossy(archive.get("word/document.xml").expect("document.xml"));
    assert!(
        doc_xml.contains("<c:chart") || doc_xml.contains("chart1.xml"),
        "document.xml must reference chart: {doc_xml}"
    );

    // Verify round-trip through import_docx
    let reimported = import_docx(&docx_bytes).expect("import docx with chart");
    let reimported_chart_block = reimported
        .blocks
        .iter()
        .find(|b| b.kind == CHART_BLOCK_KIND)
        .expect("Reimported document must contain chart block");
    let reimported_chart = Chart::from_json(reimported_chart_block.text.as_str())
        .expect("Reimported chart json must parse");
    assert_eq!(
        reimported_chart.title.as_deref(),
        Some("Quarterly Performance")
    );
    assert_eq!(reimported_chart.series.len(), 2);
    assert_eq!(reimported_chart.categories, vec!["Q1", "Q2", "Q3", "Q4"]);

    // Verify PDF export contains chart title and text
    let pdf_bytes = export_pdf(&current);
    let readable_pdf = loom_pdf::inspect::readable_content(&pdf_bytes).expect("readable PDF");
    assert!(
        readable_pdf.contains("Quarterly Performance"),
        "PDF export must contain chart title: {readable_pdf}"
    );
}

#[test]
fn test_word_com_rich_fixture_roundtrip() {
    let temp_dir = std::env::temp_dir().join(format!("loom-word-com-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let docx_path = temp_dir.join("office_authored.docx");
    let docx_path_str = docx_path.to_str().unwrap().replace('\\', "/");

    let script = format!(
        r#"
$word = New-Object -ComObject Word.Application
$word.Visible = $false
try {{
    $doc = $word.Documents.Add()
    
    # 1. Heading 1
    $p1 = $doc.Paragraphs.Add()
    $p1.Range.Text = "Executive Summary"
    $p1.Range.Style = "Heading 1"
    $p1.Range.InsertParagraphAfter()
    
    # 2. Body paragraph with text
    $p2 = $doc.Paragraphs.Add()
    $p2.Range.Text = "This is a comprehensive report authored in Microsoft Word."
    $p2.Range.InsertParagraphAfter()
    
    # 3. List item
    $p3 = $doc.Paragraphs.Add()
    $p3.Range.Text = "Key Performance Metric"
    $p3.Range.Style = "List Bullet"
    $p3.Range.InsertParagraphAfter()
    
    # 4. Table (2 rows x 3 columns)
    $table = $doc.Tables.Add($doc.Paragraphs.Add().Range, 2, 3)
    $table.Cell(1, 1).Range.Text = "Metric"
    $table.Cell(1, 2).Range.Text = "Target"
    $table.Cell(1, 3).Range.Text = "Actual"
    $table.Cell(2, 1).Range.Text = "Growth"
    $table.Cell(2, 2).Range.Text = "15%"
    $table.Cell(2, 3).Range.Text = "18%"
    
    $doc.SaveAs2('{docx_path_str}', 16) # 16 = wdFormatXMLDocument (.docx)
    $doc.Close()
    Write-Host "WORD_COM_AUTHOR_SUCCESS"
}} finally {{
    $word.Quit()
    [System.Runtime.InteropServices.Marshal]::ReleaseComObject($word) | Out-Null
}}
"#
    );

    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output()
        .expect("run powershell Word COM");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success() && stdout.contains("WORD_COM_AUTHOR_SUCCESS"),
        "Word COM authoring failed: status: {:?}, stdout: {}, stderr: {}",
        output.status,
        stdout,
        stderr
    );

    let docx_bytes = std::fs::read(&docx_path).expect("read office-authored docx");
    let imported = import_docx(&docx_bytes).expect("import office-authored docx");

    // 1. Assert Heading survived
    let heading_block = imported
        .blocks
        .iter()
        .find(|b| b.kind == "heading1" || b.text.as_str().contains("Executive Summary"))
        .expect("Executive Summary heading must survive");
    assert!(heading_block.text.as_str().contains("Executive Summary"));

    // 2. Assert Body paragraph survived
    let body_block = imported
        .blocks
        .iter()
        .find(|b| {
            b.text
                .as_str()
                .contains("comprehensive report authored in Microsoft Word")
        })
        .expect("Body paragraph must survive");
    assert!(!body_block.text.as_str().is_empty());

    // 3. Assert Table survived
    let table_block = imported
        .blocks
        .iter()
        .find(|b| b.kind == TABLE_BLOCK_KIND)
        .expect("Table block must survive");
    let table_text = table_block.text.as_str();
    assert!(
        table_text.contains("Metric")
            && table_text.contains("Target")
            && table_text.contains("Actual")
    );
    assert!(
        table_text.contains("Growth") && table_text.contains("15%") && table_text.contains("18%")
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_committed_word_roundtrip_fixture() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_path = manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
        .join("word_roundtrip_fixture.docx");

    assert!(
        fixture_path.exists(),
        "Committed Word fixture must exist at {}",
        fixture_path.display()
    );

    let docx_bytes = std::fs::read(&fixture_path).expect("read committed word fixture");

    // 1. Unsupported warnings check
    let warnings = loom_writer_core::inspect_docx_unsupported(&docx_bytes)
        .expect("inspect committed word fixture");
    assert!(
        warnings
            .iter()
            .any(|w| w.to_lowercase().contains("footnote")),
        "Committed Word fixture with footnotes must produce footnote warning: {warnings:?}"
    );

    // 2. Import check
    let imported = import_docx(&docx_bytes).expect("import committed word fixture");

    // Heading survived
    assert!(
        imported.blocks.iter().any(|b| b
            .text
            .as_str()
            .contains("Loom Writer Interoperability Report")),
        "Heading 1 must survive import"
    );

    // Table survived
    let table_block = imported
        .blocks
        .iter()
        .find(|b| b.kind == TABLE_BLOCK_KIND)
        .expect("Table block must survive import");
    let table_text = table_block.text.as_str();
    assert!(
        table_text.contains("Item")
            && table_text.contains("Category")
            && table_text.contains("Status"),
        "Table header must survive: {table_text}"
    );
    assert!(
        table_text.contains("DocxParser") && table_text.contains("Verified"),
        "Table content must survive: {table_text}"
    );
}
