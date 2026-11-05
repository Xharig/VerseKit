# Verse-Kit — companion for Star Citizen.
# Copyright (C) 2026 Xharig
#
# SPDX-License-Identifier: GPL-3.0-only
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, version 3.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program.  If not, see <https://www.gnu.org/licenses/>.

<#
.SYNOPSIS
Exports the committed tree of a branch into the clone of the private test build repository, checks it and
pushes it as one commit without history.

.DESCRIPTION
1. Takes the tree of -Ref as it is committed (git archive), nothing of the working tree.
2. Empties the target clone except .git, unpacks the tree, leaves out every workflow of the tree and puts back
   the workflow of the target (.github/workflows/testbau.yml); the workflows in $Carried come along from the
   tree.
3. Turns the version X.Y.Z-dev of the tree into X.Y.Z-rc<Rc> (Cargo.toml and the workspace crates in
   Cargo.lock); the workflow builds the installer with the version of Cargo.toml.
4. Refuses files that never belong there and checks .gitignore.
5. Makes one commit without parents and runs tools/privat_spuren_pruefen.py over it.
6. Runs the test no_key (no archive key in the code or in the p4kx sources of the build).
7. Pushes over the old commit (--force), unless -NoPush.

Run:  pwsh tools/testbau_export.ps1 -Rc <n> [-Ref rust] [-Target <clone>] [-NoPush] [-SkipNoKey]
      pwsh tools/testbau_export.ps1 -SelfTest     the version change on samples and on the files of -Ref
                                                   (X.Y.Z-dev: its test version; X.Y.Z: the release -Final builds)
Exit code: 0 pushed (or ready with -NoPush), 1 a check failed, 2 the export itself failed.
#>

[CmdletBinding()]
param(
    [ValidateRange(1, 9999)]
    [int]$Rc,
    [string]$Ref = 'rust',
    [string]$Target = '',
    [string]$Remote = 'https://github.com/Xharig/VerseKit-Testbau.git',
    [switch]$NoPush,
    [switch]$SkipNoKey,
    # Builds the finished version X.Y.Z of the tree as it is, without -rc.
    [switch]$Final,
    [switch]$SelfTest
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$Source = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not $Target) { $Target = Join-Path (Split-Path $Source -Parent) 'VerseKit-Testbau' }
$Workflow = '.github/workflows/testbau.yml'

# Paths of the tree that do not go into the test build repository.
$LeftOut = @('.github/workflows')

# Workflows of the tree that go into the test build repository all the same.
$Carried = @('.github/workflows/linux-pruefung.yml')

# Names that never belong into the export, wherever they lie.
$Forbidden = @('.env', '.env.*', '*.pem', '*.key', '*.p12', '*.pfx', 'id_rsa*', 'id_ed25519*', 'spuren.json',
    '.sperrliste', 'CLAUDE.md', 'AGENTS.md', '*UEBERGABE*.md')

# Patterns of the trace guard whose hits may stand in the private repository, by the SHA-256 of their regex:
# traces of the development (the SSH line of the deploy key, folder and page names of the developer tools), no
# personal data.
$AcceptedPatterns = @(
    '86d4afc69cc2085b01afab4e6a5203c95fb5542489f25ae4bf03f8f09d761cd9',
    '90fc308f822c272658cc4578c6605c53591c2827251f4a1265a31b6c9ba01714',
    '6e33fe23254b707a05b18935d891129ab0486988281d16bc8a86600b2def13d7'
)

# Lines .gitignore has to hold, so that a build or the local notes never get committed.
$IgnoreLines = @('/target', 'CLAUDE.md', '.claude/', 'dist/')

function Fail([int]$Code, [string]$Text) {
    Write-Host "FEHLER: $Text" -ForegroundColor Red
    exit $Code
}

function Invoke-Git([string]$Repo, [string[]]$GitArgs) {
    $out = & git.exe -C $Repo @GitArgs 2>&1
    if ($LASTEXITCODE -ne 0) { Fail 2 "git $($GitArgs -join ' '): $out" }
    return $out
}

# The version of a test build: X.Y.Z-rcN. The error report server takes it, the updater treats it as a program
# file (not as a build of the source like -dev), and the finished X.Y.Z stands above it.
function Get-TestVersion([string]$Version, [int]$Number) {
    if ($Version -notmatch '^(\d{1,4}\.\d{1,4}\.\d{1,4})-dev$') { throw "Die Version im Baum ist '$Version', erwartet X.Y.Z-dev." }
    return "$($Matches[1])-rc$Number"
}

# The version under [workspace.package] of a Cargo.toml.
function Get-WorkspaceVersion([string]$Toml) {
    $section = [regex]::Match($Toml, '(?ms)^\[workspace\.package\][ \t]*\r?$(.*?)(?=^\[|\z)')
    if (-not $section.Success) { throw 'Cargo.toml hat keinen Abschnitt [workspace.package].' }
    $line = [regex]::Matches($section.Groups[1].Value, '(?m)^version[ \t]*=[ \t]*"([^"]+)"')
    if ($line.Count -ne 1) { throw "[workspace.package] hat $($line.Count) Zeilen version, erwartet eine." }
    return $line[0].Groups[1].Value
}

# The finished version X.Y.Z of a Cargo.toml, built as it is with -Final; anything with a suffix is refused.
function Get-FinalVersion([string]$Toml) {
    $version = Get-WorkspaceVersion $Toml
    if ($version -notmatch '^\d{1,4}\.\d{1,4}\.\d{1,4}$') { throw "Die Version im Baum ist '$version', keine fertige X.Y.Z." }
    return $version
}

# The packages without a source (the crates of the workspace) in a Cargo.lock that carry $Version.
function Get-LockMembers([string]$Lock, [string]$Version) {
    $blocks = [regex]::Split($Lock, '(?m)^(?=\[\[package\]\])')
    return @($blocks | Where-Object {
        $_.StartsWith('[[package]]') -and $_ -notmatch '(?m)^source[ \t]*=' -and
        $_ -match ('(?m)^version = "' + [regex]::Escape($Version) + '"\r?$')
    }).Count
}

# Cargo.toml and Cargo.lock with the test version: the version of [workspace.package], and in the lock every
# package without a source (the crates of the workspace) that carries the old version. Nothing else changes.
function Set-TestVersion([string]$Toml, [string]$Lock, [int]$Number) {
    $old = Get-WorkspaceVersion $Toml
    $new = Get-TestVersion $old $Number
    $section = [regex]::Match($Toml, '(?ms)^\[workspace\.package\][ \t]*\r?$(.*?)(?=^\[|\z)').Groups[1]
    $body = [regex]::Replace($section.Value, '(?m)^(version[ \t]*=[ \t]*")[^"]+(")', "`${1}$new`${2}")
    $toml2 = $Toml.Substring(0, $section.Index) + $body + $Toml.Substring($section.Index + $section.Length)

    $members = 0
    $blocks = [regex]::Split($Lock, '(?m)^(?=\[\[package\]\])')
    $lock2 = -join ($blocks | ForEach-Object {
        $block = $_
        if ($block.StartsWith('[[package]]') -and $block -notmatch '(?m)^source[ \t]*=' -and
            $block -match ('(?m)^version = "' + [regex]::Escape($old) + '"\r?$')) {
            $members++
            $block = [regex]::Replace($block, ('(?m)^version = "' + [regex]::Escape($old) + '"'), "version = `"$new`"")
        }
        $block
    })
    if ($members -eq 0) { throw "Cargo.lock hat kein Paket des Workspace mit der Version $old." }
    return [pscustomobject]@{ Old = $old; New = $new; Toml = $toml2; Lock = $lock2; Members = $members }
}

if ($SelfTest) {
    $failed = 0
    function Check([string]$Name, [bool]$Ok) {
        if ($Ok) { Write-Host "ok    $Name" } else { Write-Host "FEHLT $Name" -ForegroundColor Red; $script:failed++ }
    }
    function Refused([scriptblock]$Call) {
        try { & $Call | Out-Null; return $false } catch { return $true }
    }

    $toml = "[workspace]`nmembers = [`"crates/*`"]`n`n[workspace.package]`nversion = `"5.0.0-dev`"`nedition = `"2024`"`n`n" +
        "[workspace.dependencies]`nbase64 = `"0.22`"`nimage = { version = `"5.0.0-dev`" }`n`n" +
        "[workspace.metadata.probe]`nversion = `"5.0.0-dev`"`n"
    $lock = "version = 4`n`n[[package]]`nname = `"base64`"`nversion = `"5.0.0-dev`"`nsource = `"registry+https://github.com/rust-lang/crates.io-index`"`n`n" +
        "[[package]]`nname = `"versekit-app`"`nversion = `"5.0.0-dev`"`ndependencies = [`n `"base64`",`n]`n`n" +
        "[[package]]`nname = `"versekit-core`"`nversion = `"5.0.0-dev`"`n"
    $r = Set-TestVersion $toml $lock 3
    Check 'Version X.Y.Z-dev wird X.Y.Z-rc3' ($r.Old -eq '5.0.0-dev' -and $r.New -eq '5.0.0-rc3')
    Check 'Cargo.toml: nur [workspace.package] geaendert' ($r.Toml -eq $toml.Replace("[workspace.package]`nversion = `"5.0.0-dev`"", "[workspace.package]`nversion = `"5.0.0-rc3`""))
    Check 'Cargo.lock: zwei Pakete des Workspace, das aus der Registry nicht' ($r.Members -eq 2 -and
        $r.Lock -eq $lock.Replace("versekit-app`"`nversion = `"5.0.0-dev`"", "versekit-app`"`nversion = `"5.0.0-rc3`"").Replace("versekit-core`"`nversion = `"5.0.0-dev`"", "versekit-core`"`nversion = `"5.0.0-rc3`""))
    $crlf = Set-TestVersion ($toml -replace "`n", "`r`n") ($lock -replace "`n", "`r`n") 12
    Check 'CRLF bleibt CRLF' ($crlf.Members -eq 2 -and
        $crlf.Toml -eq ($r.Toml.Replace('rc3', 'rc12') -replace "`n", "`r`n") -and
        $crlf.Lock -eq ($r.Lock.Replace('rc3', 'rc12') -replace "`n", "`r`n"))
    Check 'nie -test.N, immer -rcN' ((Get-TestVersion '5.0.1-dev' 9999) -eq '5.0.1-rc9999')
    Check 'abgelehnt: Version ohne -dev' (Refused { Set-TestVersion ($toml.Replace('5.0.0-dev"' + "`nedition", '5.0.0"' + "`nedition")) $lock 1 })
    Check 'abgelehnt: Version schon -rc' (Refused { Get-TestVersion '5.0.0-rc1' 2 })
    Check 'abgelehnt: kein [workspace.package]' (Refused { Set-TestVersion ($toml.Replace('[workspace.package]', '[package]')) $lock 1 })
    Check 'abgelehnt: zwei Zeilen version' (Refused { Set-TestVersion ($toml.Replace('edition', "version = `"1.0.0`"`nedition")) $lock 1 })
    Check 'abgelehnt: kein Paket des Workspace im Lock' (Refused { Set-TestVersion $toml ($lock -replace 'versekit-(app|core)"\nversion = "5\.0\.0-dev"', 'x"' + "`nversion = `"1.0.0`"") 1 })
    $range = (Get-Command $PSCommandPath).Parameters['Rc'].Attributes | Where-Object { $_ -is [System.Management.Automation.ValidateRangeAttribute] }
    Check '-Rc nur 1 bis 9999 (der Fehlerbericht-Server nimmt hoechstens vier Ziffern)' ($range -and $range.MinRange -eq 1 -and $range.MaxRange -eq 9999)

    # -Final: the finished X.Y.Z only, unchanged.
    $finishedToml = $toml.Replace('5.0.0-dev"' + "`nedition", '5.0.2"' + "`nedition")
    Check '-Final: fertige Version 5.0.2 wird genommen' ((Get-FinalVersion $finishedToml) -eq '5.0.2')
    Check '-Final: andere Zeile vor version' ((Get-FinalVersion "[workspace.package]`nedition = `"2024`"`nversion = `"5.0.2`"`n") -eq '5.0.2')
    Check '-Final abgelehnt: Version -dev' (Refused { Get-FinalVersion $toml })
    Check '-Final abgelehnt: Version -rc' (Refused { Get-FinalVersion ($toml.Replace('5.0.0-dev"' + "`nedition", '5.0.2-rc1"' + "`nedition")) })
    Check 'Pakete des Workspace im Lock gezaehlt, das aus der Registry nicht' ((Get-LockMembers $lock '5.0.0-dev') -eq 2)

    # The files of -Ref: a tree with X.Y.Z-dev gets its test version; a released tree (X.Y.Z, built with -Final)
    # gets none, and every crate in the lock has to carry the version.
    $realToml = (Invoke-Git $Source @('show', "${Ref}:Cargo.toml")) -join "`n"
    $realLock = (Invoke-Git $Source @('show', "${Ref}:Cargo.lock")) -join "`n"
    $crates = @(Invoke-Git $Source @('ls-tree', '--name-only', "${Ref}:crates")).Count
    $realVersion = Get-WorkspaceVersion $realToml
    if ($realVersion -match '-dev$') {
        $real = Set-TestVersion $realToml $realLock 1
        Check "echte Dateien von ${Ref}: $($real.Old) wird $($real.New)" ((Get-WorkspaceVersion $real.Toml) -eq $real.New)
        Check "echte Dateien von ${Ref}: $($real.Members) Pakete im Lock = $crates Crates" ($real.Members -eq $crates)
        Check "echte Dateien von ${Ref}: kein $($real.Old) mehr" (-not $real.Toml.Contains($real.Old) -and -not $real.Lock.Contains("`"$($real.Old)`""))
    } else {
        $finished = try { Get-FinalVersion $realToml } catch { $null }
        Check "echte Dateien von ${Ref}: $realVersion ist eine fertige Version (nur mit -Final)" ($finished -eq $realVersion)
        Check "echte Dateien von ${Ref}: kein Testbau ohne -dev" (Refused { Set-TestVersion $realToml $realLock 1 })
        $members = Get-LockMembers $realLock $realVersion
        Check "echte Dateien von ${Ref}: $members Pakete im Lock mit $realVersion = $crates Crates" ($members -eq $crates)
    }
    if ($failed) { Write-Host "$failed Pruefung(en) rot" -ForegroundColor Red; exit 1 }
    Write-Host 'Selbsttest gruen'
    exit 0
}
if (-not $Final -and -not $PSBoundParameters.ContainsKey('Rc')) { Fail 2 'Die Nummer des Testbaus fehlt: -Rc <n> (ergibt X.Y.Z-rc<n>).' }

# ---------------------------------------------------------------- source and target
$sha = (Invoke-Git $Source @('rev-parse', '--verify', "$Ref^{commit}")).Trim()
$short = $sha.Substring(0, 10)
Write-Host "Quelle: $Ref @ $short"

$targetFull = [System.IO.Path]::GetFullPath($Target)
if ($targetFull.TrimEnd('\', '/') -eq $Source.TrimEnd('\', '/') -or $targetFull.StartsWith($Source + [System.IO.Path]::DirectorySeparatorChar)) {
    Fail 2 "Das Ziel $targetFull liegt in der Quelle."
}
if (-not (Test-Path (Join-Path $targetFull '.git'))) { Fail 2 "$targetFull ist kein Git-Klon (erst anlegen, siehe Vault)." }
$origin = (Invoke-Git $targetFull @('remote', 'get-url', 'origin')).Trim()
if ($origin -ne $Remote) { Fail 2 "origin von $targetFull ist '$origin', erwartet '$Remote'." }

$workflowFile = Join-Path $targetFull $Workflow
if (-not (Test-Path $workflowFile)) {
    # A run that stopped between emptying and unpacking leaves the workflow only in the last commit.
    & git.exe -C $targetFull cat-file -e "HEAD:$Workflow" 2>$null
    if ($LASTEXITCODE -ne 0) { Fail 2 "$Workflow fehlt im Ziel; der Workflow wird nur dort gepflegt." }
    & git.exe -C $targetFull checkout HEAD -- $Workflow
    if ($LASTEXITCODE -ne 0) { Fail 2 "$Workflow liess sich nicht aus dem letzten Commit holen." }
}
$workflowText = [System.IO.File]::ReadAllBytes($workflowFile)

function Restore-Workflow {
    New-Item -ItemType Directory -Force (Split-Path $workflowFile -Parent) | Out-Null
    [System.IO.File]::WriteAllBytes($workflowFile, $workflowText)
}

# ---------------------------------------------------------------- export
$tar = Join-Path ([System.IO.Path]::GetTempPath()) "versekit-testbau-$short.tar"
Invoke-Git $Source @('archive', '--format=tar', '-o', $tar, $sha) | Out-Null

# The build folder of no_key stays (ignored by .gitignore), everything else goes.
Get-ChildItem -LiteralPath $targetFull -Force | Where-Object Name -notin @('.git', 'target') | Remove-Item -Recurse -Force
Restore-Workflow
# The tar of Windows: the GNU tar of Git for Windows reads "C:" as a host name.
$tarExe = if ($env:SystemRoot -and (Test-Path "$env:SystemRoot\System32\tar.exe")) { "$env:SystemRoot\System32\tar.exe" } else { 'tar' }
& $tarExe -xf $tar -C $targetFull
if ($LASTEXITCODE -ne 0) { Fail 2 "tar ended with $LASTEXITCODE" }
Remove-Item $tar -Force

$carriedText = @{}
foreach ($path in $Carried) {
    $full = Join-Path $targetFull $path
    if (Test-Path $full) { $carriedText[$path] = [System.IO.File]::ReadAllBytes($full) }
    else { Write-Host "Nicht im Baum von ${Ref}: $path" }
}
foreach ($path in $LeftOut) {
    $full = Join-Path $targetFull $path
    if (Test-Path $full) { Remove-Item $full -Recurse -Force }
}
Restore-Workflow
foreach ($path in $carriedText.Keys) {
    [System.IO.File]::WriteAllBytes((Join-Path $targetFull $path), $carriedText[$path])
}

# ---------------------------------------------------------------- test version
$tomlFile = Join-Path $targetFull 'Cargo.toml'
$lockFile = Join-Path $targetFull 'Cargo.lock'
$noBom = [System.Text.UTF8Encoding]::new($false)
if ($Final) {
    try { $version = Get-FinalVersion ([System.IO.File]::ReadAllText($tomlFile)) } catch { Fail 1 $_.Exception.Message }
    Write-Host "Version: $version (fertig, unveraendert)"
} else {
    try {
        $changed = Set-TestVersion ([System.IO.File]::ReadAllText($tomlFile)) ([System.IO.File]::ReadAllText($lockFile)) $Rc
    } catch { Fail 1 $_.Exception.Message }
    [System.IO.File]::WriteAllText($tomlFile, $changed.Toml, $noBom)
    [System.IO.File]::WriteAllText($lockFile, $changed.Lock, $noBom)
    $version = $changed.New
    Write-Host "Version: $($changed.Old) -> $version ($($changed.Members) Pakete im Cargo.lock)"
}

# ---------------------------------------------------------------- forbidden files and .gitignore
$tracked = Get-ChildItem -LiteralPath $targetFull -Force | Where-Object Name -notin @('.git', 'target')
$hits = foreach ($name in $Forbidden) {
    $tracked | Where-Object Name -like $name
    $tracked | Where-Object PSIsContainer | Get-ChildItem -Recurse -Force -Filter $name -ErrorAction SilentlyContinue
}
if ($hits) { Fail 1 ("Verbotene Dateien im Export:`n  " + (($hits | ForEach-Object FullName) -join "`n  ")) }

$ignore = Get-Content (Join-Path $targetFull '.gitignore') -Encoding utf8
foreach ($line in $IgnoreLines) {
    if ($ignore -notcontains $line) { Fail 1 ".gitignore hat die Zeile '$line' nicht." }
}

# ---------------------------------------------------------------- one commit without history
Invoke-Git $targetFull @('config', 'user.name', 'Xharig') | Out-Null
Invoke-Git $targetFull @('config', 'user.email', '238144048+Xharig@users.noreply.github.com') | Out-Null
$branch = "export-$short"
Invoke-Git $targetFull @('checkout', '--quiet', '--orphan', $branch) | Out-Null
Invoke-Git $targetFull @('add', '--all') | Out-Null
$message = Join-Path ([System.IO.Path]::GetTempPath()) "versekit-testbau-$short.txt"
[System.IO.File]::WriteAllText($message, "Test build $version of $Ref at $short`n", $noBom)
Invoke-Git $targetFull @('commit', '--quiet', '-F', $message) | Out-Null
Remove-Item $message -Force
Invoke-Git $targetFull @('branch', '--quiet', '-M', $branch, 'main') | Out-Null
$files = (Invoke-Git $targetFull @('ls-files')).Count
Write-Host "Export: ein Commit, $files Dateien"

$untracked = Invoke-Git $targetFull @('status', '--porcelain', '--ignored')
if ($untracked) { Write-Host "Nicht im Commit (ignoriert):`n$($untracked -join "`n")" }

# ---------------------------------------------------------------- private traces
$guard = & python -I (Join-Path $targetFull 'tools/privat_spuren_pruefen.py') --repo $targetFull --ref HEAD 2>&1
$guardCode = $LASTEXITCODE
$guard | ForEach-Object { Write-Host $_ }
switch ($guardCode) {
    0 { Write-Host 'Spurenwächter: sauber' }
    1 {
        # Hits of the accepted patterns only are fine for the private repository; any other pattern stops.
        $patternFile = if ($env:VERSEKIT_SPUREN) { $env:VERSEKIT_SPUREN } else { Join-Path $env:LOCALAPPDATA 'VerseKit-Waechter\spuren.json' }
        $patterns = (Get-Content $patternFile -Raw -Encoding utf8 | ConvertFrom-Json).muster
        $sha256 = [System.Security.Cryptography.SHA256]::Create()
        $hit = @($guard | ForEach-Object { if ("$_" -match '^\s*Muster (\d+):') { [int]$Matches[1] } } | Sort-Object -Unique)
        $other = @($hit | Where-Object {
            $bytes = [System.Text.Encoding]::UTF8.GetBytes($patterns[$_ - 1].regex)
            $hash = -join ($sha256.ComputeHash($bytes) | ForEach-Object { $_.ToString('x2') })
            $AcceptedPatterns -notcontains $hash
        })
        if (-not $hit -or $other) { Fail 1 "Der Spurenwächter hat Spuren gefunden (Muster $($other -join ', ')); nichts gepusht." }
        Write-Host "Spurenwächter: nur Entwicklerspuren (Muster $($hit -join ', ')), im privaten Repo erlaubt"
    }
    default { Fail 1 "Der Spurenwächter konnte nicht alles prüfen (Code $guardCode); nichts gepusht." }
}

# ---------------------------------------------------------------- no archive key
if (-not $SkipNoKey) {
    Push-Location $targetFull
    try {
        & cargo test --locked -p versekit-gamedata --test no_key
        if ($LASTEXITCODE -ne 0) { Fail 1 'no_key ist rot; nichts gepusht.' }
    } finally { Pop-Location }
}

# ---------------------------------------------------------------- push
if ($NoPush) {
    Write-Host "Bereit, nicht gepusht (-NoPush). Pushen: git -C `"$targetFull`" push --force origin main"
    exit 0
}
Invoke-Git $targetFull @('push', '--force', 'origin', 'main') | Out-Null
$remoteHead = ((Invoke-Git $targetFull @('ls-remote', 'origin', 'refs/heads/main')) -split '\s+')[0]
$localHead = (Invoke-Git $targetFull @('rev-parse', 'HEAD')).Trim()
if ($remoteHead -ne $localHead) { Fail 2 "Push nicht angekommen: origin/main $remoteHead, lokal $localHead." }
Write-Host "Gepusht: $localHead ($version aus $Ref @ $short)"
Write-Host 'Bauen: gh workflow run testbau.yml -R Xharig/VerseKit-Testbau'
exit 0
