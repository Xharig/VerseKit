# VerseKit — companion for Star Citizen.
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
Builds the test build (the feature test-build of versekit-app) into a target folder of its own.

.DESCRIPTION
The test build works on the real data and the real game folders like the build players get, and also on the test
channels. It never replaces the build players get: it is built into its own target folder (-Target, default
%USERPROFILE%\VerseKit-Testfassung), and the file to start is <Target>\release\versekit.exe, started without
--daten. The two builds never run at the same time; the second one says so and ends.

.PARAMETER Target
The target folder of cargo for the test build.

.PARAMETER Jobs
How many compile jobs cargo may use.
#>
param(
    [string]$Target = (Join-Path $env:USERPROFILE 'VerseKit-Testfassung'),
    [int]$Jobs = 4
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$env:CARGO_TARGET_DIR = $Target
$env:CARGO_BUILD_JOBS = "$Jobs"
Push-Location $repo
try {
    cargo build --release --locked -p versekit-app --features test-build
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" }
}
finally {
    Pop-Location
}
Write-Output (Join-Path $Target 'release\versekit.exe')
