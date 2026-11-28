# VerseKit — companion for Star Citizen.
# Copyright (C) 2026 Xharig
# SPDX-License-Identifier: GPL-3.0-only
#
# Builds the Stream Deck plugin as release and writes the plugin folder and the installable file
# VerseKit.streamDeckPlugin into the given folder (default: target\streamdeck). The release build of VerseKit takes
# the file from there and puts it beside its program.
param([string]$Target = "")

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
if ($Target -eq "") { $Target = Join-Path $root "target\streamdeck" }
New-Item -ItemType Directory -Force -Path $Target | Out-Null

# Throttled while the game runs.
if (Get-Process -Name StarCitizen -ErrorAction SilentlyContinue) { $env:CARGO_BUILD_JOBS = "2" }

Push-Location $root
try {
    cargo build --release --locked -p versekit-streamdeck
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    $cargoTarget = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root "target" }
    $exe = Join-Path $cargoTarget "release\versekit-streamdeck.exe"
    $run = Start-Process -FilePath $exe -ArgumentList @("--paket", $Target) -Wait -PassThru -NoNewWindow
    if ($run.ExitCode -ne 0) { throw "packing failed ($($run.ExitCode))" }
    $file = Join-Path $Target "VerseKit.streamDeckPlugin"
    if (-not (Test-Path $file)) { throw "no plugin file written" }
    Get-Item $file | Select-Object FullName, Length
} finally {
    Pop-Location
}
