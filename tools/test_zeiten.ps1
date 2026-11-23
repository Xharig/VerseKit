# Misst die Laufzeit jedes Tests des Pflichtlaufs von VerseKit v5 und die Gesamtzeit.
#
# Aufruf (aus dem Arbeitsordner):  pwsh tools/test_zeiten.ps1 [-Ausgabe <Ordner>] [-NurLauf]
#
# - Läuft Star Citizen, wird gedrosselt (obere 8 Kerne, Priorität Idle, 4 Bau-Jobs, 4 Test-Threads),
#   sonst 8 Bau-Jobs und 8 Test-Threads.
# - Baut erst (`--no-run`, Zeit getrennt), führt dann jedes Testprogramm für sich aus und schreibt je Test
#   `Name<TAB>Sekunden<TAB>Ergebnis<TAB>Programm` in `zeiten.tsv`.
# - `RUSTC_BOOTSTRAP=1` schaltet nur die Zeitmeldung (`--report-time`) der Testprogramme frei, ohne Nightly.
# - Ändert nichts am Quelltext, fasst keine echten Daten an.

param(
    [string]$Ausgabe = (Join-Path $PSScriptRoot '..\target\test-zeiten'),
    [switch]$NurLauf,
    # Nur diese Testprogramme (z. B. versekit_core) und nur Tests mit diesen Namensteilen
    [string[]]$Nur,
    [string[]]$Ohne,
    [string[]]$Filter
)

$ErrorActionPreference = 'Stop'
$wurzel = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Set-Location $wurzel
New-Item -ItemType Directory -Force $Ausgabe | Out-Null
$Ausgabe = (Resolve-Path $Ausgabe).Path

$spiel = [bool](Get-Process -Name 'StarCitizen' -ErrorAction SilentlyContinue)
if ($spiel) {
    $self = Get-Process -Id $PID
    $self.ProcessorAffinity = [IntPtr]0xFF00
    $self.PriorityClass = 'Idle'
    $env:CARGO_BUILD_JOBS = '4'
    $threads = 4
} else {
    $env:CARGO_BUILD_JOBS = '8'
    $threads = 8
}
$env:RUSTC_BOOTSTRAP = '1'
"Star Citizen läuft: $spiel; Jobs=$($env:CARGO_BUILD_JOBS); Test-Threads=$threads" | Tee-Object (Join-Path $Ausgabe 'lauf.txt')

# Pflichtlauf: alles im Release-Profil (wie die Oberflächen-Tests der Regel in CLAUDE.md), ohne `#[ignore]`.
$stoppuhr = [Diagnostics.Stopwatch]::StartNew()
$bauzeit = $null
if (-not $NurLauf) {
    $meldung = & cargo test --release --workspace --no-run --message-format=json 2>$null |
        Where-Object { $_ -like '*"compiler-artifact"*' -and $_ -like '*"executable":"*' }
    $bauzeit = $stoppuhr.Elapsed.TotalSeconds
    $meldung | Set-Content (Join-Path $Ausgabe 'artefakte.jsonl')
    "Bau: $([math]::Round($bauzeit)) s" | Tee-Object (Join-Path $Ausgabe 'lauf.txt') -Append
}

$programme = Get-Content (Join-Path $Ausgabe 'artefakte.jsonl') | ForEach-Object { $_ | ConvertFrom-Json } |
    Where-Object { $_.profile.test -eq $true -and $_.executable } |
    ForEach-Object { [pscustomobject]@{ Name = $_.target.name; Art = ($_.target.kind -join ','); Pfad = $_.executable; Quelle = $_.target.src_path } }
if ($Nur) { $programme = $programme | Where-Object { $Nur -contains $_.Name } }
if ($Ohne) { $programme = $programme | Where-Object { $Ohne -notcontains $_.Name } }
$filter = @($Filter)

$tsv = Join-Path $Ausgabe 'zeiten.tsv'
'Test`tSekunden`tErgebnis`tProgramm' -replace '`t', "`t" | Set-Content $tsv
$gesamt = @()
foreach ($p in $programme) {
    # Wie `cargo test`: Arbeitsordner und Umgebung des Crates, sonst läuft ein Test auf der echten Maschine.
    $ordner = Split-Path $p.Quelle
    while ($ordner -and -not (Test-Path (Join-Path $ordner 'Cargo.toml'))) { $ordner = Split-Path $ordner }
    $env:VERSEKIT_TESTUMGEBUNG = Join-Path $wurzel 'target\testumgebung'
    $env:CARGO_MANIFEST_DIR = $ordner
    $t0 = [Diagnostics.Stopwatch]::StartNew()
    Push-Location $ordner
    $roh = & $p.Pfad @filter --test-threads=$threads -Z unstable-options --report-time 2>&1
    Pop-Location
    $dauer = $t0.Elapsed.TotalSeconds
    $roh | Set-Content (Join-Path $Ausgabe ("roh-$($p.Name).txt"))
    $zahl = 0
    foreach ($z in $roh) {
        if ("$z" -match '^test (\S+) \.\.\. (ok|FAILED|ignored)(?:, .*)? <([\d.]+)s>') {
            Add-Content $tsv "$($Matches[1])`t$($Matches[3])`t$($Matches[2])`t$($p.Name)"
            $zahl++
        }
    }
    $gesamt += [pscustomobject]@{ Programm = $p.Name; Art = $p.Art; Sekunden = [math]::Round($dauer, 1); Tests = $zahl }
    "$($p.Name) [$($p.Art)]: $([math]::Round($dauer, 1)) s, $zahl Tests" | Tee-Object (Join-Path $Ausgabe 'lauf.txt') -Append
}
$gesamt | Sort-Object Sekunden -Descending | Export-Csv (Join-Path $Ausgabe 'programme.csv') -NoTypeInformation
"Lauf gesamt (ohne Bau): $([math]::Round(($gesamt | Measure-Object Sekunden -Sum).Sum)) s; mit Bau: $([math]::Round($stoppuhr.Elapsed.TotalSeconds)) s" |
    Tee-Object (Join-Path $Ausgabe 'lauf.txt') -Append
