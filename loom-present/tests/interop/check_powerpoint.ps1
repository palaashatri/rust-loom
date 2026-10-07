<#
  Gate 11 interoperability check for Loom Present, using real Microsoft PowerPoint.

  Opens the files written by
    LOOM_INTEROP_OUT=<dir> cargo test -p loom-present-core --test interop_fixtures -- --ignored
  (present.pptx, present.pdf) through COM, read-only and without a window, and
  prints one PASS/FAIL/INFO line per check. Exit code 1 when any check FAILs.

  Usage: powershell -NoProfile -ExecutionPolicy Bypass -File check_powerpoint.ps1 -Dir <fixture dir>

  Fixture files are never modified: the presentation opens read-only and the
  PDF PowerPoint writes goes to a temporary file that is deleted afterwards.
  Only the PowerPoint instance started here is quit, and stopped by process id
  if Quit did not end it.

  COM note: paths passed to PowerPoint are plain [string] values; a
  PSObject-wrapped string (what Join-Path returns) can hang Office calls.
#>
param([string]$Dir = $env:LOOM_INTEROP_OUT)

$ErrorActionPreference = 'Stop'
if (-not $Dir) { throw 'pass -Dir or set LOOM_INTEROP_OUT' }
$Dir = [string](Resolve-Path -LiteralPath $Dir).Path

$script:fails = 0
$script:passes = 0
function Check([string]$name, [bool]$ok, [string]$detail = '') {
    if ($ok) { $script:passes++; Write-Output "PASS  $name" }
    else { $script:fails++; Write-Output "FAIL  $name  -- $detail" }
}
function Info([string]$text) { Write-Output "INFO  $text" }
function Try-Get([scriptblock]$Block) { try { & $Block } catch { $null } }

function Read-PdfText([byte[]]$bytes) {
    # Minimal reader for the uncompressed PDFs Loom writes: page count, media box,
    # embedded image count and the text strings in drawing order (WinAnsi bytes
    # re-decoded as Windows-1252; PDF string escapes resolved).
    $raw = [System.Text.Encoding]::GetEncoding(28591).GetString($bytes)
    $win = [System.Text.Encoding]::GetEncoding(1252)
    $strings = @()
    foreach ($m in [regex]::Matches($raw, '\(((?:\\.|[^\\)])*)\) Tj')) {
        $text = [regex]::Replace($m.Groups[1].Value, '\\([0-7]{1,3}|.)', {
            param($e)
            $v = $e.Groups[1].Value
            if ($v -match '^[0-7]{1,3}$') { [string][char][Convert]::ToInt32($v, 8) }
            elseif ($v -eq 'n') { "`n" } else { $v }
        })
        $strings += $win.GetString([System.Text.Encoding]::GetEncoding(28591).GetBytes($text))
    }
    [pscustomobject]@{
        Raw = $raw; Header = $raw.Substring(0, 8)
        Pages = ([regex]::Matches($raw, '/Type /Page[^s]')).Count
        Images = ([regex]::Matches($raw, '/Subtype\s*/Image')).Count
        Strings = $strings
    }
}

$subtitle = "Prepared by Finance $([char]0x2013) Caf$([char]0xE9) $([char]0x201C)review$([char]0x201D)"
$pdfPath = [string](Join-Path $Dir 'present.pdf')
$pptxPath = [string](Join-Path $Dir 'present.pptx')

# ---- present.pdf: structure and text, decoded directly (no application needed)
$pdf = Read-PdfText ([System.IO.File]::ReadAllBytes($pdfPath))
Check 'present.pdf is a PDF with 4 landscape pages (842 x 595)' ($pdf.Header.StartsWith('%PDF-') -and ($pdf.Pages -eq 4) -and $pdf.Raw.Contains('/MediaBox [0 0 842.00 595.00]')) "header '$($pdf.Header)' pages $($pdf.Pages)"
foreach ($t in 'Q3 & Q4 Plan', 'Results', 'Box label', 'Circle label', '42%', 'Logo', 'Thank you') {
    Check "present.pdf text: $t" ($pdf.Strings -contains $t) ''
}
Check 'present.pdf subtitle keeps the en dash, accent and curly quotes' ($pdf.Strings -contains $subtitle) "strings: $($pdf.Strings -join ' | ')"
Check 'present.pdf embeds the picture' ($pdf.Images -eq 1) "images $($pdf.Images)"
$multi = @($pdf.Strings | Where-Object { $_ -like '*First line*' })
Check 'present.pdf breaks the two-line body text into two lines' (($multi.Count -ge 1) -and (($multi | Where-Object { $_ -like '*Second line*' }).Count -eq 0)) "body drawn as: $($multi -replace "`n", '<LF>')"

$before = @(Get-Process -Name POWERPNT -ErrorAction SilentlyContinue | ForEach-Object { $_.Id })
$ppt = New-Object -ComObject PowerPoint.Application
Start-Sleep -Milliseconds 800
# PowerPoint has no reliable window handle without a window: the instance started here is the new POWERPNT process.
$pptPid = [uint32](@(Get-Process -Name POWERPNT -ErrorAction SilentlyContinue | Where-Object { $before -notcontains $_.Id } | ForEach-Object { $_.Id }) | Select-Object -First 1)
$pres = $null; $tempPdf = $null
try {
    $ppt.DisplayAlerts = 1   # ppAlertsNone
    Info "PowerPoint $($ppt.Version) build $($ppt.Build)"
    # Open(FileName, ReadOnly = msoTrue, Untitled = msoFalse, WithWindow = msoFalse)
    $pres = $ppt.Presentations.Open($pptxPath, -1, 0, 0)
    Check 'pptx opens in PowerPoint read-only without repair' ($pres.Name -eq 'present.pptx') "name '$($pres.Name)'"
    Check 'four slides' ($pres.Slides.Count -eq 4) "slides $($pres.Slides.Count)"
    Check 'slide size is 16:9 (960 x 540 pt)' (([math]::Abs($pres.PageSetup.SlideWidth - 960) -lt 0.5) -and ([math]::Abs($pres.PageSetup.SlideHeight - 540) -lt 0.5)) "$($pres.PageSetup.SlideWidth) x $($pres.PageSetup.SlideHeight)"

    function Shape-Text($shape) { if ($shape.HasTextFrame) { return [string]$shape.TextFrame.TextRange.Text } return '' }
    function Texts($slide) { $r = @(); foreach ($sh in $slide.Shapes) { $t = Shape-Text $sh; if ($t) { $r += ($t -replace "[\r\v]", "`n") } } return $r }

    $s1 = Texts $pres.Slides.Item(1); $s2 = Texts $pres.Slides.Item(2); $s3 = Texts $pres.Slides.Item(3); $s4 = Texts $pres.Slides.Item(4)
    Check 'slide 1 text: title and subtitle (accents, dash, curly quotes)' (($s1 -contains 'Q3 & Q4 Plan') -and ($s1 -contains $subtitle)) "texts: $($s1 -join ' | ')"
    Check 'slide 2 text: Results, body lines, shape labels, 42%' (($s2 -contains 'Results') -and ($s2 -contains "First line`nSecond line") -and ($s2 -contains 'Box label') -and ($s2 -contains 'Circle label') -and ($s2 -contains '42%')) "texts: $($s2 -join ' | ')"
    Check 'slide 3 text: Logo' ($s3 -contains 'Logo') "texts: $($s3 -join ' | ')"
    Check 'slide 4 text: Thank you' ($s4 -contains 'Thank you') "texts: $($s4 -join ' | ')"
    Check 'slide 1 has the title placeholder with the title text' ($pres.Slides.Item(1).Shapes.HasTitle -and ($pres.Slides.Item(1).Shapes.Title.TextFrame.TextRange.Text -eq 'Q3 & Q4 Plan')) ''

    # ---- shapes: type, geometry, rotation
    $rect = $null; $ellipse = $null
    foreach ($sh in $pres.Slides.Item(2).Shapes) { if ($sh.Name -eq 'rect2') { $rect = $sh }; if ($sh.Name -eq 'circle2') { $ellipse = $sh } }
    Check 'rectangle shape is an auto shape (rectangle) rotated 15 degrees' (($null -ne $rect) -and ($rect.Type -eq 1) -and ($rect.AutoShapeType -eq 1) -and ([math]::Abs($rect.Rotation - 15) -lt 0.5)) "type $($rect.Type) auto $($rect.AutoShapeType) rotation $($rect.Rotation)"
    Check 'circle shape is an oval' (($null -ne $ellipse) -and ($ellipse.AutoShapeType -eq 9)) "auto $($ellipse.AutoShapeType)"
    # authoring plane 1000 x 562.5 -> 960 x 540 pt: rect2 is at (600, 140) size 300 x 120
    Check 'rectangle geometry (576, 134.4, 288 x 115.2 pt)' (($null -ne $rect) -and ([math]::Abs($rect.Left - 576) -lt 1) -and ([math]::Abs($rect.Top - 134.4) -lt 1) -and ([math]::Abs($rect.Width - 288) -lt 1) -and ([math]::Abs($rect.Height - 115.2) -lt 1)) "$($rect.Left),$($rect.Top) $($rect.Width)x$($rect.Height)"

    # ---- picture
    $pictures = @(); foreach ($sh in $pres.Slides.Item(3).Shapes) { if ($sh.Type -eq 13) { $pictures += $sh } }
    Check 'slide 3 has exactly one picture shape' ($pictures.Count -eq 1) "pictures $($pictures.Count)"
    if ($pictures.Count -eq 1) {
        $pic = $pictures[0]
        Check 'picture keeps the 2:1 aspect of the 120 x 60 source' ([math]::Abs($pic.Width / $pic.Height - 2) -lt 0.02) "$($pic.Width) x $($pic.Height)"
        Check 'picture is centred on the slide' (([math]::Abs($pic.Left + $pic.Width / 2 - 480) -lt 2) -and ([math]::Abs($pic.Top + $pic.Height / 2 - 270) -lt 2)) "centre $($pic.Left + $pic.Width / 2),$($pic.Top + $pic.Height / 2)"
    }
    $total = 0; foreach ($sl in $pres.Slides) { foreach ($sh in $sl.Shapes) { if ($sh.Type -eq 13) { $total++ } } }
    Check 'only slide 3 has a picture' ($total -eq 1) "pictures in deck $total"

    # ---- speaker notes
    function Notes($slide) { $r = ''; foreach ($sh in $slide.NotesPage.Shapes) { if ($sh.HasTextFrame -and $sh.Type -eq 14 -and $sh.PlaceholderFormat.Type -eq 2) { $r = [string]$sh.TextFrame.TextRange.Text } } return $r }
    Check 'slide 1 notes' ((Notes $pres.Slides.Item(1)) -eq 'Open warmly. Mention the budget.') "notes '$(Notes $pres.Slides.Item(1))'"
    Check 'slide 2 notes' ((Notes $pres.Slides.Item(2)) -eq 'Walk through the numbers slowly.') "notes '$(Notes $pres.Slides.Item(2))'"
    Check 'slides 3 and 4 have no notes' (((Notes $pres.Slides.Item(3)) -eq '') -and ((Notes $pres.Slides.Item(4)) -eq '')) ''

    # ---- transitions (PpEntryEffect: 0 none, 1793 fade, 3849 fade smoothly, 3852-3855 push down/left/right/up)
    $effects = @(); foreach ($n in 1..4) { $effects += [int]$pres.Slides.Item($n).SlideShowTransition.EntryEffect }
    Info "EntryEffect per slide: $($effects -join ', ')"
    $fadeKinds = 1793, 3849
    Check 'slide 1 transition is a fade (Dissolve)' ($fadeKinds -contains $effects[0]) "effect $($effects[0])"
    Check 'slide 2 transition is a push' (($effects[1] -ge 3852) -and ($effects[1] -le 3855)) "effect $($effects[1])"
    Check 'slide 3 has no transition' ($effects[2] -eq 0) "effect $($effects[2])"
    Check 'slide 4 transition is a fade (Morph is written as a fade)' ($fadeKinds -contains $effects[3]) "effect $($effects[3])"
    # ---- PowerPoint exports the pptx to PDF with one page per slide
    $tempPdf = [string]([IO.Path]::GetTempPath() + 'loom-interop-ppt-' + [Guid]::NewGuid().ToString('N') + '.pdf')
    $pres.SaveCopyAs($tempPdf, 32)   # ppSaveAsPDF
    $exists = Test-Path $tempPdf
    Check 'PowerPoint exports the pptx to a non-empty PDF' ($exists -and ((Get-Item $tempPdf).Length -gt 1000)) ''
    if ($exists) {
        $head = [System.Text.Encoding]::ASCII.GetString([System.IO.File]::ReadAllBytes($tempPdf), 0, 5)
        Check 'the PDF PowerPoint wrote is a PDF' ($head -eq '%PDF-') "header '$head'"
    }
    $pres.Close(); $pres = $null
}
catch {
    $script:fails++
    Write-Output "FAIL  unexpected error  -- $($_.Exception.Message) at line $($_.InvocationInfo.ScriptLineNumber)"
}
finally {
    if ($pres) { try { $pres.Close() } catch {} }
    try { $ppt.Quit() } catch {}
    foreach ($o in $pres, $ppt) { if ($o) { try { [void][Runtime.InteropServices.Marshal]::ReleaseComObject($o) } catch {} } }
    [GC]::Collect(); [GC]::WaitForPendingFinalizers()
    Start-Sleep -Milliseconds 800
    if ($pptPid -ne 0 -and (Get-Process -Id $pptPid -ErrorAction SilentlyContinue)) { Stop-Process -Id $pptPid -Force -ErrorAction SilentlyContinue }
    if ($tempPdf) { Remove-Item -LiteralPath $tempPdf -ErrorAction SilentlyContinue }
}
Write-Output "SUMMARY powerpoint: $script:passes passed, $script:fails failed"
if ($script:fails -gt 0) { exit 1 }
