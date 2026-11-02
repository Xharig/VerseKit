# VerseKit - fetches the Hangar-Exporter for the installer and checks it.
# Copyright (C) 2026 Xharig
# SPDX-License-Identifier: GPL-3.0-only
#
# Fills dist\erweiterung\ with the signed Firefox file of the Hangar-Exporter, its licence, its copyright
# notice and the address of its source. Stops with an error when anything does not match:
#   1. the SHA-256 of the file against the value written below (not against a file fetched with it),
#   2. every file in the package against its SHA-256 in META-INF/manifest.mf, and the manifest against
#      META-INF/mozilla.sf (no file missing, none added),
#   3. the signing certificate: the add-on id as subject, issued by Mozilla.
#
#   pwsh packaging\hangar-exporter.ps1                  fetch from the release and check
#   pwsh packaging\hangar-exporter.ps1 -Xpi <file>      check a file at hand
#   pwsh packaging\hangar-exporter.ps1 -SelfTest        the checks once with the real file and twice with a
#                                                       changed copy, which has to fail

param(
    [string]$Xpi = '',
    [string]$Out = (Join-Path $PSScriptRoot '..\dist\erweiterung'),
    [string]$Expected = '09da458dd779982656e0322170a7a866b620bf62d1db308ae8b44b6f9e38d60b',
    [switch]$SelfTest
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
Add-Type -AssemblyName System.Security

$Version = '0.5.0'
$FileName = 'ddb5c5ab59c744a0acca-0.5.0.xpi'
$Release = "https://github.com/Drachennarr/Hangar-Exporter-Releases/releases/tag/v$Version"
$Download = "https://github.com/Drachennarr/Hangar-Exporter-Releases/releases/download/v$Version/$FileName"
$AddonId = 'hangar-exporter@dragonkeep.invalid'

function Get-Sha256Base64([byte[]]$Bytes) {
    [Convert]::ToBase64String([System.Security.Cryptography.SHA256]::HashData($Bytes))
}

function Read-Entry($Entry) {
    $stream = $Entry.Open()
    try {
        $memory = [System.IO.MemoryStream]::new()
        $stream.CopyTo($memory)
        return , $memory.ToArray()
    } finally {
        $stream.Dispose()
    }
}

# The `Name:` sections of a manifest as name -> SHA256-Digest.
function Read-Manifest([string]$Text) {
    $table = @{}
    foreach ($section in ($Text -replace "`r", '') -split "`n`n") {
        $name = $null
        $digest = $null
        foreach ($line in $section -split "`n") {
            if ($line.StartsWith('Name: ')) { $name = $line.Substring(6) }
            if ($line.StartsWith('SHA256-Digest: ')) { $digest = $line.Substring(15) }
        }
        if ($name) {
            if ($table.ContainsKey($name)) { throw "manifest.mf names $name twice" }
            $table[$name] = $digest
        }
    }
    return $table
}

# All checks on one file; throws at the first that fails.
function Test-Xpi([string]$Path, [string]$Want) {
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
    if ($actual -ne $Want.ToLowerInvariant()) {
        throw "SHA-256 of $Path is $actual, expected $Want"
    }
    $zip = [System.IO.Compression.ZipFile]::OpenRead($Path)
    try {
        $entries = @{}
        foreach ($entry in $zip.Entries) {
            if ($entry.FullName.EndsWith('/')) { continue }
            $entries[$entry.FullName] = $entry
        }
        foreach ($needed in 'META-INF/manifest.mf', 'META-INF/mozilla.sf', 'META-INF/mozilla.rsa', 'LICENSE', 'COPYRIGHT.txt') {
            if (-not $entries.ContainsKey($needed)) { throw "$needed is missing in the package" }
        }
        $manifestBytes = Read-Entry $entries['META-INF/manifest.mf']
        $manifest = Read-Manifest ([System.Text.Encoding]::UTF8.GetString($manifestBytes))
        $content = $entries.Keys | Where-Object { -not $_.StartsWith('META-INF/') }
        foreach ($name in $content) {
            if (-not $manifest.ContainsKey($name)) { throw "$name is not in manifest.mf" }
        }
        foreach ($name in $manifest.Keys) {
            if (-not $entries.ContainsKey($name)) { throw "$name of manifest.mf is missing in the package" }
            $digest = Get-Sha256Base64 (Read-Entry $entries[$name])
            if ($digest -ne $manifest[$name]) { throw "$name does not match its digest in manifest.mf" }
        }
        $sf = [System.Text.Encoding]::UTF8.GetString((Read-Entry $entries['META-INF/mozilla.sf']))
        $line = ($sf -replace "`r", '') -split "`n" | Where-Object { $_.StartsWith('SHA256-Digest-Manifest: ') } | Select-Object -First 1
        if (-not $line -or $line.Substring(24) -ne (Get-Sha256Base64 $manifestBytes)) {
            throw 'manifest.mf does not match mozilla.sf'
        }
        $cms = [System.Security.Cryptography.Pkcs.SignedCms]::new()
        $cms.Decode((Read-Entry $entries['META-INF/mozilla.rsa']))
        $signer = $cms.Certificates | Where-Object { $_.Subject -match [regex]::Escape("CN=$AddonId") } | Select-Object -First 1
        if (-not $signer) { throw "no certificate for $AddonId in mozilla.rsa" }
        if ($signer.Issuer -notmatch 'Mozilla') { throw "the certificate is not issued by Mozilla: $($signer.Issuer)" }
        return $manifest.Count
    } finally {
        $zip.Dispose()
    }
}

# The licence and the copyright notice from the checked package.
function Export-Entry([string]$Path, [string]$Name, [string]$Target) {
    $zip = [System.IO.Compression.ZipFile]::OpenRead($Path)
    try {
        [System.IO.File]::WriteAllBytes($Target, (Read-Entry ($zip.Entries | Where-Object { $_.FullName -eq $Name })))
    } finally {
        $zip.Dispose()
    }
}

function Get-Xpi {
    if ($Xpi) { return (Resolve-Path -LiteralPath $Xpi).Path }
    $folder = Join-Path ([System.IO.Path]::GetTempPath()) "versekit-hangar-exporter-$PID"
    New-Item -ItemType Directory -Force -Path $folder | Out-Null
    $target = Join-Path $folder $FileName
    Invoke-WebRequest -Uri $Download -OutFile $target -UseBasicParsing
    return $target
}

if ($SelfTest) {
    $real = Get-Xpi
    $count = Test-Xpi $real $Expected
    Write-Output "real file: passes ($count files checked)"
    $folder = Join-Path ([System.IO.Path]::GetTempPath()) "versekit-hangar-gegenprobe-$PID"
    New-Item -ItemType Directory -Force -Path $folder | Out-Null
    try {
        # A changed byte at the end: the checksum written above has to fail.
        $changed = Join-Path $folder 'geaendert.xpi'
        $bytes = [System.IO.File]::ReadAllBytes($real)
        $bytes[$bytes.Length - 1] = $bytes[$bytes.Length - 1] -bxor 1
        [System.IO.File]::WriteAllBytes($changed, $bytes)
        $failed = $false
        try { Test-Xpi $changed $Expected | Out-Null } catch { $failed = $true; Write-Output "changed byte: fails ($($_.Exception.Message))" }
        if (-not $failed) { throw 'counter-check: a changed byte passed' }
        # A file in the package changed, with its own checksum given: the digests of the signature have to fail.
        $repacked = Join-Path $folder 'umgepackt.xpi'
        Copy-Item -LiteralPath $real -Destination $repacked
        $zip = [System.IO.Compression.ZipFile]::Open($repacked, 'Update')
        try {
            $entry = $zip.GetEntry('manifest.json')
            $text = [System.Text.Encoding]::UTF8.GetString((Read-Entry $entry)) + ' '
            $entry.Delete()
            $writer = [System.IO.StreamWriter]::new($zip.CreateEntry('manifest.json').Open())
            $writer.Write($text)
            $writer.Dispose()
        } finally {
            $zip.Dispose()
        }
        $own = (Get-FileHash -Algorithm SHA256 -LiteralPath $repacked).Hash
        $failed = $false
        try { Test-Xpi $repacked $own | Out-Null } catch { $failed = $true; Write-Output "changed file in the package: fails ($($_.Exception.Message))" }
        if (-not $failed) { throw 'counter-check: a changed file in the package passed' }
    } finally {
        Remove-Item -Recurse -Force -LiteralPath $folder
    }
    Write-Output 'self-test passed'
    exit 0
}

$source = Get-Xpi
$count = Test-Xpi $source $Expected
New-Item -ItemType Directory -Force -Path $Out | Out-Null
Get-ChildItem -LiteralPath $Out -File | Remove-Item -Force
Copy-Item -LiteralPath $source -Destination (Join-Path $Out "hangar-exporter-$Version.xpi")
Export-Entry $source 'LICENSE' (Join-Path $Out 'LICENSE.txt')
Export-Entry $source 'COPYRIGHT.txt' (Join-Path $Out 'COPYRIGHT.txt')
$note = @(
    "Hangar-Exporter $Version (Firefox), Copyright (C) 2026 Drachennarr, GPL-3.0-only.",
    "Quelle / Source: $Release",
    "SHA-256 ($FileName): $Expected",
    'Der Quelltext sind die lesbaren JavaScript-Dateien in der .xpi-Datei (ein ZIP-Archiv).',
    'The source code is the readable JavaScript files in the .xpi file (a ZIP archive).'
) -join "`r`n"
[System.IO.File]::WriteAllText((Join-Path $Out 'QUELLE.txt'), $note + "`r`n", [System.Text.UTF8Encoding]::new($false))
Write-Output "Hangar-Exporter $Version checked ($count files) and placed in $((Resolve-Path -LiteralPath $Out).Path)"
