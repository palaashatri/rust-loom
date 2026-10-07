<#
  Gate 11 interoperability check for Loom Writer, using real Microsoft Word.

  Opens the files written by
    LOOM_INTEROP_OUT=<dir> cargo test -p loom-writer-core --test interop_fixtures -- --ignored
  (writer.docx, writer.pdf, writer.md) in an invisible Word through COM,
  read-only, with alerts off, and prints one PASS/FAIL/INFO line per check.
  Exit code 1 when any check FAILs.

  Usage: powershell -NoProfile -ExecutionPolicy Bypass -File check_word.ps1 -Dir <fixture dir>

  Fixture files are never modified: documents open read-only and the PDF
  export of the docx goes to a temporary file that is deleted afterwards.
  Only the Word instance started here is quit, and stopped by process id if
  Quit did not end it.
#>
param([string]$Dir = $env:LOOM_INTEROP_OUT)

$ErrorActionPreference = 'Stop'
if (-not $Dir) { throw 'pass -Dir or set LOOM_INTEROP_OUT' }
$Dir = (Resolve-Path -LiteralPath $Dir).Path


$script:fails = 0
$script:passes = 0
function Check([string]$name, [bool]$ok, [string]$detail = '') {
    if ($ok) { $script:passes++; Write-Output "PASS  $name" }
    else { $script:fails++; Write-Output "FAIL  $name  -- $detail" }
}
function Info([string]$text) { Write-Output "INFO  $text" }
function Try-Get([scriptblock]$Block) { try { & $Block } catch { $null } }
function Clean([string]$text) { return ($text -replace "[\r\a\u0007]", '').Trim() }

$cafe = "Caf$([char]0xE9) Zo$([char]0xEB) $([char]0x2013) $([char]0x201C)quoted$([char]0x201D) $([char]0x2014) $([char]0x20AC)5 $([char]0x2026) D$([char]0xFC)sseldorf"

# ---- Markdown file: plain text checks, no application needed
$mdPath = Join-Path $Dir 'writer.md'
$md = [System.IO.File]::ReadAllText($mdPath, (New-Object System.Text.UTF8Encoding($false, $true)))
Check 'markdown: heading 1 and heading 2 use # and ##' (($md -match '(?m)^# Quarterly Overview$') -and ($md -match '(?m)^## Details$')) ''
Check 'markdown: bulleted and numbered lists' (($md -match '(?m)^- Apples$') -and ($md -match '(?m)^- Pears$') -and ($md -match '(?m)^1\. First step$') -and ($md -match '(?m)^3\. Third step$')) ''
Check 'markdown: table rows' (($md -match '(?m)^\| Name \| Qty \|$') -and ($md -match '(?m)^\| --- \| --- \|$') -and ($md -match '(?m)^\| Coffee \| 3 \|$')) ''
Check 'markdown: accented and typographic text is valid UTF-8' ($md.Contains($cafe)) ''
Check 'markdown: bold and italic runs keep inline markup' (($md -match '\*\*bold\*\*') -and ($md -match '(\*italic\*|_italic_)')) 'the paragraph is written as plain text; bold and italic are dropped'
Check 'markdown: a blank line separates a list from the following table' ($md -match "(?s)Third step\r?\n\r?\n\| Name") 'the table follows "3. Third step" directly, so Markdown readers treat it as part of the list item'

function Read-PdfText([byte[]]$bytes) {
    # Minimal reader for the uncompressed PDFs Loom writes: text runs in drawing order with the
    # font in force, and the page count. Latin-1 decoding keeps bytes 1:1; WinAnsi text is then
    # re-decoded as Windows-1252.
    $raw = [System.Text.Encoding]::GetEncoding(28591).GetString($bytes)
    $fonts = @{}
    foreach ($m in [regex]::Matches($raw, '(?s)(\d+) 0 obj\s*<<[^>]*?/Type /Font[^>]*?/BaseFont /([A-Za-z-]+)')) { $fonts[$m.Groups[1].Value] = $m.Groups[2].Value }
    $names = @{}
    foreach ($m in [regex]::Matches($raw, '/(F\d+) (\d+) 0 R')) { $names[$m.Groups[1].Value] = $fonts[$m.Groups[2].Value] }
    $runs = @()
    $win = [System.Text.Encoding]::GetEncoding(1252)
    foreach ($m in [regex]::Matches($raw, '/(F\d+) [\d.]+ Tf BT ([\d.\-]+) ([\d.\-]+) Td \(((?:\\.|[^\\)])*)\) Tj')) {
        $text = [regex]::Replace($m.Groups[4].Value, '\\([0-7]{1,3}|.)', {
            param($e)
            $v = $e.Groups[1].Value
            if ($v -match '^[0-7]{1,3}$') { [string][char][Convert]::ToInt32($v, 8) } else { $v }
        })
        $text = $win.GetString([System.Text.Encoding]::GetEncoding(28591).GetBytes($text))
        $runs += [pscustomobject]@{ Font = $names[$m.Groups[1].Value]; X = [double]$m.Groups[2].Value; Y = [double]$m.Groups[3].Value; Text = $text }
    }
    [pscustomobject]@{ Raw = $raw; Header = $raw.Substring(0, 8); Pages = ([regex]::Matches($raw, '/Type /Page[^s]')).Count; Runs = $runs }
}
$before = @(Get-Process -Name WINWORD -ErrorAction SilentlyContinue | ForEach-Object { $_.Id })
$word = New-Object -ComObject Word.Application
Start-Sleep -Milliseconds 500
# Word has no window handle to ask for, so the instance started here is the new WINWORD process.
$wordPid = [uint32](@(Get-Process -Name WINWORD -ErrorAction SilentlyContinue | Where-Object { $before -notcontains $_.Id } | ForEach-Object { $_.Id }) | Select-Object -First 1)
$doc = $null; $tempPdf = $null
try {
    $word.Visible = $false
    $word.DisplayAlerts = 0
    Info "Word $($word.Version) build $($word.Build)"

    $doc = $word.Documents.Open((Join-Path $Dir 'writer.docx'), $false, $true, $false)
    Check 'docx opens in Word read-only without repair' ($doc.Name -eq 'writer.docx') "name '$($doc.Name)'"

    # ---- Word can export the docx to PDF (layout and fonts resolve)
    # A plain [string]: a PSObject-wrapped path (what Join-Path returns) makes ExportAsFixedFormat hang.
    $tempPdf = [string]([IO.Path]::GetTempPath() + 'loom-interop-word-' + [Guid]::NewGuid().ToString('N') + '.pdf')
    Info "exporting the docx to PDF through Word"
    $doc.ExportAsFixedFormat($tempPdf, 17)   # wdExportFormatPDF
    Info "export done"
    Check 'Word exports the docx to a non-empty PDF' ((Test-Path $tempPdf) -and ((Get-Item $tempPdf).Length -gt 1000)) ''
    Info "Word pages for the docx: $(Try-Get { $doc.ComputeStatistics(2) })"

    # ---- paragraph text (table cell paragraphs are checked through the table)
    $texts = @(); $inTable = @{}
    $i = 0
    foreach ($p in $doc.Paragraphs) {
        $i++
        $isCell = [bool]$p.Range.Information(12)   # wdWithInTable
        $inTable[$i] = $isCell
        if (-not $isCell) { $texts += (Clean $p.Range.Text) }
    }
    $expectedTexts = @('Quarterly Overview', 'Plain bold italic underline mix', 'Details', 'Apples', 'Pears', 'First step', 'Second step', 'Third step', $cafe, 'Centered closing line')
    $nonEmpty = @($texts | Where-Object { $_ -ne '' })
    Check 'body paragraphs match, in order' (($nonEmpty -join '|') -eq ($expectedTexts -join '|')) "Word reads: $($nonEmpty -join ' | ')"
    Check 'accented and typographic characters survive' ($texts -contains $cafe) ''

    function Para([string]$text) {
        foreach ($p in $doc.Paragraphs) { if ((Clean $p.Range.Text) -eq $text) { return $p } }
        return $null
    }

    # ---- heading styles
    $h1 = Para 'Quarterly Overview'; $h2 = Para 'Details'; $body = Para 'Plain bold italic underline mix'
    Check 'Quarterly Overview uses style Heading 1 (outline level 1)' (($h1.Style.NameLocal -eq 'Heading 1') -and ($h1.OutlineLevel -eq 1)) "style '$($h1.Style.NameLocal)' outline $($h1.OutlineLevel)"
    Check 'Details uses style Heading 2 (outline level 2)' (($h2.Style.NameLocal -eq 'Heading 2') -and ($h2.OutlineLevel -eq 2)) "style '$($h2.Style.NameLocal)' outline $($h2.OutlineLevel)"
    Check 'body paragraph is body text (outline level 10)' ($body.OutlineLevel -eq 10) "outline $($body.OutlineLevel)"
    Check 'heading font is larger than body font' ($h1.Range.Font.Size -gt $body.Range.Font.Size) "h1 $($h1.Range.Font.Size) pt, body $($body.Range.Font.Size) pt"
    Check 'headings are bold' ([bool]($h1.Range.Font.Bold) -and [bool]($h2.Range.Font.Bold)) ''
    Info "fonts: body '$($body.Range.Font.Name)' $($body.Range.Font.Size) pt, heading '$($h1.Range.Font.Name)' $($h1.Range.Font.Size) pt"
    Check 'body font is a named face' (-not [string]::IsNullOrWhiteSpace($body.Range.Font.Name)) ''

    # ---- inline runs: bold 6..10, italic 11..17, underline 18..27
    $start = $body.Range.Start
    $boldRange = $doc.Range($start + 6, $start + 10); $italicRange = $doc.Range($start + 11, $start + 17); $underRange = $doc.Range($start + 18, $start + 27)
    Check 'run "bold" is bold only' (($boldRange.Text -eq 'bold') -and ($boldRange.Font.Bold -eq -1) -and ($boldRange.Font.Italic -eq 0)) "text '$($boldRange.Text)' bold $($boldRange.Font.Bold)"
    Check 'run "italic" is italic only' (($italicRange.Text -eq 'italic') -and ($italicRange.Font.Italic -eq -1) -and ($italicRange.Font.Bold -eq 0)) "text '$($italicRange.Text)' italic $($italicRange.Font.Italic)"
    Check 'run "underline" is underlined' (($underRange.Text -eq 'underline') -and ($underRange.Font.Underline -ne 0)) "text '$($underRange.Text)' underline $($underRange.Font.Underline)"
    $plainRange = $doc.Range($start, $start + 5)
    Check 'run "Plain" has no bold, italic or underline' (($plainRange.Font.Bold -eq 0) -and ($plainRange.Font.Italic -eq 0) -and ($plainRange.Font.Underline -eq 0)) ''

    # ---- lists: wdListBullet = 2, wdListSimpleNumbering = 3
    $apples = Para 'Apples'; $pears = Para 'Pears'
    Check 'Apples and Pears are a bulleted list' (($apples.Range.ListFormat.ListType -eq 2) -and ($pears.Range.ListFormat.ListType -eq 2)) "types $($apples.Range.ListFormat.ListType), $($pears.Range.ListFormat.ListType)"
    $numbers = @(); foreach ($t in 'First step', 'Second step', 'Third step') { $p = Para $t; $numbers += $p.Range.ListFormat.ListString; if ($p.Range.ListFormat.ListType -ne 3) { $numbers += "type $($p.Range.ListFormat.ListType)" } }
    Check 'steps are a numbered list 1. 2. 3.' (($numbers -join '|') -eq '1.|2.|3.') "list strings $($numbers -join '|')"
    Check 'plain paragraphs are not list items' (($body.Range.ListFormat.ListType -eq 0) -and ($h1.Range.ListFormat.ListType -eq 0)) ''

    # ---- table
    Check 'one table' ($doc.Tables.Count -eq 1) "tables $($doc.Tables.Count)"
    if ($doc.Tables.Count -ge 1) {
        $t = $doc.Tables.Item(1)
        $cells = @(); foreach ($r in 1..3) { foreach ($c in 1..2) { $cells += (Clean $t.Cell($r, $c).Range.Text) } }
        Check 'table is 3 rows by 2 columns' (($t.Rows.Count -eq 3) -and ($t.Columns.Count -eq 2)) "rows $($t.Rows.Count) columns $($t.Columns.Count)"
        Check 'table cell text: Name, Qty / Tea, 2 / Coffee, 3' (($cells -join '|') -eq 'Name|Qty|Tea|2|Coffee|3') ($cells -join '|')
        Check 'table header row is bold' ([bool]($t.Rows.Item(1).Range.Font.Bold)) ''
        Check 'table has visible borders' ($t.Borders.Enable -ne 0) ''
    }

    # ---- alignment
    $centered = Para 'Centered closing line'
    Check 'closing line is centered' ($centered.Alignment -eq 1) "alignment $($centered.Alignment)"
    Check 'body paragraph is left aligned' ($body.Alignment -eq 0) "alignment $($body.Alignment)"

    # ---- page setup: Letter landscape, Moderate margins (72 top/bottom, 54 left/right)
    $ps = $doc.PageSetup
    Check 'page is Letter landscape (792 x 612 pt)' (($ps.Orientation -eq 1) -and ([math]::Abs($ps.PageWidth - 792) -lt 1) -and ([math]::Abs($ps.PageHeight - 612) -lt 1)) "orientation $($ps.Orientation) $($ps.PageWidth) x $($ps.PageHeight)"
    Check 'margins 72 / 72 / 54 / 54 pt' (([math]::Abs($ps.TopMargin - 72) -lt 1) -and ([math]::Abs($ps.BottomMargin - 72) -lt 1) -and ([math]::Abs($ps.LeftMargin - 54) -lt 1) -and ([math]::Abs($ps.RightMargin - 54) -lt 1)) "$($ps.TopMargin)/$($ps.BottomMargin)/$($ps.LeftMargin)/$($ps.RightMargin)"

    # ---- comments
    Check 'one Word comment' ($doc.Comments.Count -eq 1) "comments $($doc.Comments.Count)"
    if ($doc.Comments.Count -ge 1) {
        $cm = $doc.Comments.Item(1)
        Check 'comment author and text' (($cm.Author -eq 'Ada Lovelace') -and ((Clean $cm.Range.Text) -eq 'Check this word')) "author '$($cm.Author)' text '$(Clean $cm.Range.Text)'"
        Check 'comment is anchored on the word "bold"' ((Clean $cm.Scope.Text) -eq 'bold') "scope '$(Clean $cm.Scope.Text)'"
    }

    # ---- document properties
    # BuiltInDocumentProperties is late bound; PowerShell needs reflection to reach Item/Value.
    $binding = [System.Reflection.BindingFlags]::GetProperty
    $props = $doc.BuiltInDocumentProperties
    $titleProp = [System.__ComObject].InvokeMember('Item', $binding, $null, $props, @('Title'))
    $title = [string][System.__ComObject].InvokeMember('Value', $binding, $null, $titleProp, $null)
    Check 'document title property is "Interop Report"' ($title -eq 'Interop Report') "title '$title'"

    $doc.Close($false); $doc = $null

    # ---- Loom's own PDF. Word's PDF reflow hangs an unattended Word (it hangs on a PDF Word
    # wrote itself), so the PDF is checked by decoding its uncompressed content stream.
    $pdfPath = Join-Path $Dir 'writer.pdf'
    $pdfBytes = [System.IO.File]::ReadAllBytes($pdfPath)
    $pdf = Read-PdfText $pdfBytes
    Check 'writer.pdf is a PDF with one Letter-landscape page (792 x 612)' ($pdf.Header.StartsWith('%PDF-') -and ($pdf.Pages -eq 1) -and $pdf.Raw.Contains('/MediaBox [0 0 792.00 612.00]')) "header '$($pdf.Header)' pages $($pdf.Pages)"
    Check 'writer.pdf fonts are base-14 Helvetica faces with WinAnsi encoding' (($pdf.Raw.Contains('/BaseFont /Helvetica-Bold')) -and ($pdf.Raw.Contains('/BaseFont /Helvetica-Oblique')) -and ($pdf.Raw.Contains('/Encoding /WinAnsiEncoding'))) ''
    $lines = @($pdf.Runs | ForEach-Object { $_.Text })
    Check 'writer.pdf text: heading, paragraph, list markers' (($lines -contains 'Quarterly Overview') -and ($lines -contains 'Apples') -and ($lines -contains 'First step') -and ($lines -contains 'Third step') -and ($lines -contains 'Details')) ''
    Check 'writer.pdf accented and typographic text decodes (WinAnsi)' ($lines -contains $cafe) ''
    $bold = $pdf.Runs | Where-Object { $_.Text -eq 'bold' } | Select-Object -First 1
    $ital = $pdf.Runs | Where-Object { $_.Text -eq 'italic' } | Select-Object -First 1
    Check 'writer.pdf draws "bold" in Helvetica-Bold and "italic" in Helvetica-Oblique' (($bold.Font -eq 'Helvetica-Bold') -and ($ital.Font -eq 'Helvetica-Oblique')) "bold font '$($bold.Font)', italic font '$($ital.Font)'"
    $centerRun = $pdf.Runs | Where-Object { $_.Text -eq 'Centered closing line' } | Select-Object -First 1
    Check 'writer.pdf centers the centered paragraph (x well right of the 54 pt margin)' ($centerRun.X -gt 150) "x = $($centerRun.X)"
    $pipes = @($lines | Where-Object { $_ -like '*| --- |*' -or $_ -like '| Name |*' })
    Check 'writer.pdf draws the table as a grid, not as Markdown pipe text' ($pipes.Count -eq 0) "found literal Markdown rows: $($pipes -join ' ; ')"}
catch {
    $script:fails++
    Write-Output "FAIL  unexpected error  -- $($_.Exception.Message) at line $($_.InvocationInfo.ScriptLineNumber)"
}
finally {
    if ($doc) { try { $doc.Close($false) } catch {} }
    try { $word.Quit([ref]0) } catch {}
    foreach ($o in $doc, $word) { if ($o) { try { [void][Runtime.InteropServices.Marshal]::ReleaseComObject($o) } catch {} } }
    [GC]::Collect(); [GC]::WaitForPendingFinalizers()
    Start-Sleep -Milliseconds 500
    if ($wordPid -ne 0 -and (Get-Process -Id $wordPid -ErrorAction SilentlyContinue)) { Stop-Process -Id $wordPid -Force -ErrorAction SilentlyContinue }
    if ($tempPdf) { Remove-Item -LiteralPath $tempPdf -ErrorAction SilentlyContinue }
}
Write-Output "SUMMARY word: $script:passes passed, $script:fails failed"
if ($script:fails -gt 0) { exit 1 }
