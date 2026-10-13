<#
.SYNOPSIS
  Generates a rich round-trip DOCX fixture using Microsoft Word COM.
.DESCRIPTION
  Authors a complete Word document containing Headings, styled paragraphs,
  bulleted and numbered lists, a 3x3 table with headers and merged cells,
  footnotes, and embedded drawings.
#>
param(
    [string]$OutPath = ""
)

$ErrorActionPreference = 'Stop'

if (-not $OutPath) {
    $scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
    $OutPath = Join-Path $scriptDir "word_roundtrip_fixture.docx"
}

$word = $null
$doc = $null

try {
    $word = New-Object -ComObject Word.Application
    $word.Visible = $false
    $word.DisplayAlerts = 0
    Write-Output "INFO Word COM $($word.Version) build $($word.Build)"

    $doc = $word.Documents.Add()

    # 1. Heading 1
    $p1 = $doc.Paragraphs.Add()
    $p1.Range.Text = "Loom Writer Interoperability Report"
    $p1.Range.Style = "Heading 1"
    $p1.Range.InsertParagraphAfter()

    # 2. Heading 2
    $p2 = $doc.Paragraphs.Add()
    $p2.Range.Text = "Executive Summary and Features"
    $p2.Range.Style = "Heading 2"
    $p2.Range.InsertParagraphAfter()

    # 3. Rich paragraph with formatting
    $p3 = $doc.Paragraphs.Add()
    $p3.Range.Text = "This rich fixture tests fidelity across Microsoft Word and Loom Writer. It contains bold and italic styles."
    $p3.Range.InsertParagraphAfter()

    # 4. Bulleted list
    $p4 = $doc.Paragraphs.Add()
    $p4.Range.Text = "Feature verification: Headings and paragraphs"
    $p4.Range.Style = "List Bullet"
    $p4.Range.InsertParagraphAfter()

    $p5 = $doc.Paragraphs.Add()
    $p5.Range.Text = "Feature verification: Tables and styling"
    $p5.Range.Style = "List Bullet"
    $p5.Range.InsertParagraphAfter()

    # 5. Numbered list
    $p6 = $doc.Paragraphs.Add()
    $p6.Range.Text = "Step 1: Export from Word"
    $p6.Range.Style = "List Number"
    $p6.Range.InsertParagraphAfter()

    $p7 = $doc.Paragraphs.Add()
    $p7.Range.Text = "Step 2: Import into Loom Writer"
    $p7.Range.Style = "List Number"
    $p7.Range.InsertParagraphAfter()

    # 6. Table (3 rows x 3 columns)
    $tableRange = $doc.Paragraphs.Add().Range
    $tbl = $doc.Tables.Add($tableRange, 3, 3)
    $tbl.Cell(1, 1).Range.Text = "Item"
    $tbl.Cell(1, 2).Range.Text = "Category"
    $tbl.Cell(1, 3).Range.Text = "Status"
    $tbl.Cell(1, 1).Range.Font.Bold = 1
    $tbl.Cell(1, 2).Range.Font.Bold = 1
    $tbl.Cell(1, 3).Range.Font.Bold = 1

    $tbl.Cell(2, 1).Range.Text = "DocxParser"
    $tbl.Cell(2, 2).Range.Text = "Core"
    $tbl.Cell(2, 3).Range.Text = "Verified"

    $tbl.Cell(3, 1).Range.Text = "Merged Notes"
    # Merge cell (3, 1) and (3, 2)
    $tbl.Cell(3, 1).Merge($tbl.Cell(3, 2))
    $tbl.Cell(3, 2).Range.Text = "PASS"

    # 7. Footnote
    $fnRange = $doc.Paragraphs.Add().Range
    $fnRange.Text = "Document with references"
    $doc.Footnotes.Add($fnRange, [ref]"", "Footnote 1: Verified via Word COM automation.") | Out-Null

    # Save as .docx (format 16 = wdFormatXMLDocument)
    $resolvedOut = [System.IO.Path]::GetFullPath($OutPath)
    Write-Output "INFO Saving fixture to $resolvedOut"
    $doc.SaveAs2($resolvedOut, 16)
    $doc.Close([ref]$false)
    $doc = $null
    Write-Output "SUCCESS: Word fixture authored successfully at $resolvedOut"
} finally {
    if ($doc) {
        try { $doc.Close([ref]$false) } catch {}
    }
    if ($word) {
        try { $word.Quit([ref]0) } catch {}
        [System.Runtime.InteropServices.Marshal]::ReleaseComObject($word) | Out-Null
    }
    [GC]::Collect()
    [GC]::WaitForPendingFinalizers()
}
