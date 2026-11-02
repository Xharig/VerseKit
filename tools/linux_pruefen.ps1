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
Checks the Linux build of the workspace on Windows: cargo check and clippy for x86_64-unknown-linux-gnu, with
Zig as the C compiler of the build scripts (cargo-zigbuild).

.DESCRIPTION
1. cargo check -p versekit-app
2. cargo check --workspace --exclude versekit-overlay-proto --all-targets
3. cargo clippy (same selection); red on any warning, as with -D warnings, but without stopping at the first
   crate with a finding, so that the crates above it are linted too

Every step runs, also when one before it is red. Each cargo call runs at low priority on the cores of the mask
FF00. The logs and linux-lauf.txt (findings per file) go to -Out. Nothing is linked or run: tests and the start
under a virtual screen are in .github/workflows/linux-pruefung.yml.

Needs: rustup target add x86_64-unknown-linux-gnu, Zig (winget install zig.zig), cargo install cargo-zigbuild.

Run:  pwsh tools/linux_pruefen.ps1 [-Out <folder>] [-Steps check-app,check-workspace,clippy]
Exit code: 0 all steps green, 1 a step was red, 2 a tool is missing.
#>

[CmdletBinding()]
param(
    [string]$Out = '',
    [ValidateSet('check-app', 'check-workspace', 'clippy')]
    [string[]]$Steps = @('check-app', 'check-workspace', 'clippy')
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not $Out) { $Out = Join-Path $Root 'target\linux-pruefung' }
$Target = 'x86_64-unknown-linux-gnu'
$Selection = @('--workspace', '--exclude', 'versekit-overlay-proto', '--all-targets')
$Finding = '^[^ ]+\.rs:\d+:\d+: (error|warning)'

function Fail([string]$Text) {
    Write-Host "FEHLER: $Text" -ForegroundColor Red
    exit 2
}

# ---------------------------------------------------------------- tools
$installed = & rustup target list --installed 2>&1
if ($installed -notcontains $Target) { Fail "Rust-Ziel fehlt: rustup target add $Target" }
if (-not (Get-Command cargo-zigbuild -ErrorAction SilentlyContinue)) { Fail 'cargo-zigbuild fehlt: cargo install cargo-zigbuild --locked' }
if (-not (Get-Command zig -ErrorAction SilentlyContinue)) {
    $packages = Join-Path $env:LOCALAPPDATA 'Microsoft\WinGet\Packages'
    $zig = Get-ChildItem $packages -Directory -Filter 'zig.zig_*' -ErrorAction SilentlyContinue |
        Get-ChildItem -Recurse -Filter zig.exe -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $zig) { Fail 'Zig fehlt: winget install zig.zig' }
    $env:PATH = "$($zig.DirectoryName);$env:PATH"
}
Write-Host "zig $(& zig version), $(& cargo-zigbuild --version)"

# ---------------------------------------------------------------- steps
$commands = @{
    'check-app'       = @('check', '-p', 'versekit-app')
    'check-workspace' = @('check') + $Selection
    'clippy'          = @('clippy') + $Selection
}

New-Item -ItemType Directory -Force $Out | Out-Null
$results = [ordered]@{}
Push-Location $Root
try {
    foreach ($step in $Steps) {
        $cargoArgs = $commands[$step] + @('--target', $Target, '--locked', '--message-format', 'short')
        Write-Host "== ${step}: cargo-zigbuild $($cargoArgs -join ' ')"
        $started = Get-Date
        # `start /wait` does not hand on the exit code; the started cmd prints it as the last line.
        $inner = "cargo-zigbuild $($cargoArgs -join ' ') & echo VERSEKIT_EXIT=!errorlevel!"
        $lines = @(& cmd.exe /c start '""' /b /wait /low /affinity FF00 cmd.exe /v:on /c "$inner" 2>&1 | ForEach-Object { "$_" })
        $mark = @($lines | Where-Object { $_ -match '^VERSEKIT_EXIT=-?\d+\s*$' })
        $code = if ($mark) { [int](($mark[-1] -split '=')[1]) } else { -1 }
        $lines = @($lines | Where-Object { $_ -notmatch '^VERSEKIT_EXIT=' })
        $log = Join-Path $Out "$step.log"
        [System.IO.File]::WriteAllLines($log, [string[]]@($lines), [System.Text.UTF8Encoding]::new($false))
        $found = @($lines | Where-Object { $_ -match $Finding }).Count
        $results[$step] = if ($code -ne 0) { "failure (exit $code)" }
            elseif ($step -eq 'clippy' -and $found) { "failure ($found findings)" }
            else { 'success' }
        Write-Host ("   {0}, {1} Funde, {2:mm\:ss}" -f $results[$step], $found, ((Get-Date) - $started))
    }
} finally { Pop-Location }

# ---------------------------------------------------------------- linux-lauf.txt
$all = foreach ($step in $Steps) { Get-Content (Join-Path $Out "$step.log") -Encoding utf8 }
$findings = @($all | Where-Object { $_ -match $Finding } | ForEach-Object { $_ -replace '\\', '/' } | Sort-Object -Unique)
$commit = (& git -C $Root rev-parse --short=10 HEAD).Trim()
$dirty = if (& git -C $Root status --porcelain) { ' (mit Änderungen im Arbeitsbaum)' } else { '' }

$report = [System.Collections.Generic.List[string]]::new()
$report.Add("Linux-Pruefung lokal (Zig), $commit$dirty, $(Get-Date -Format 'yyyy-MM-dd HH:mm')")
$report.Add('')
foreach ($step in $results.Keys) { $report.Add(('{0,-18}{1}' -f "${step}:", $results[$step])) }
$report.Add('')
$report.Add('== Findings of check and clippy per file ==')
$findings | Group-Object { ($_ -split ':')[0] } | Sort-Object Name | ForEach-Object { $report.Add(('{0,5} {1}' -f $_.Count, $_.Name)) }
$report.Add('')
$report.Add('== Findings of check and clippy ==')
$findings | ForEach-Object { $report.Add($_) }
$lauf = Join-Path $Out 'linux-lauf.txt'
[System.IO.File]::WriteAllLines($lauf, $report, [System.Text.UTF8Encoding]::new($false))
Write-Host "Ergebnis: $lauf ($($findings.Count) Funde)"

if ($results.Values | Where-Object { $_ -ne 'success' }) { exit 1 }
exit 0
