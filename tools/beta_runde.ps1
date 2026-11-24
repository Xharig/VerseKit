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
Makes the files of a beta release: new codes, or the key ring and the encrypted files for a list of testers.

.DESCRIPTION
-NeueCodes <Anzahl> prints fresh personal codes (until the Discord bot makes them).
-Tag, -Liste, -Aus and -Dateien encrypt the built files for the testers of the list: the folder then holds
VerseKit-Beta.vkkey, one <file>.vkbeta per file and SHA256SUMS.txt, the attachments of the beta release.

The list is JSON, [{"tester": "...", "code": "XXXX-XXXX-XXXX-XXXX-XXXX"}], and must lie OUTSIDE the repository. Codes
and names are never printed and never passed on the command line; only the path of the list is.

.EXAMPLE
tools\beta_runde.ps1 -NeueCodes 5

.EXAMPLE
tools\beta_runde.ps1 -Tag beta-v5.7.0-beta.1 -Liste D:\Beta\tester.json -Aus D:\Beta\runde1 -Dateien dist\VerseKit-Setup.exe
#>
param(
    [int]$NeueCodes = 0,
    [string]$Tag,
    [string]$Liste,
    [string]$Aus,
    [string[]]$Dateien,
    # Only for trying it out: the least Argon2id cost, so that a check runs quickly.
    [switch]$Schnell
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$cargo = @('run', '--quiet', '--release', '--locked', '-p', 'versekit-core', '--example', 'beta_verschluesseln', '--')

Push-Location $repo
try {
    if ($NeueCodes -gt 0) {
        & cargo @cargo neue-codes $NeueCodes
    }
    else {
        if (-not $Tag -or -not $Liste -or -not $Aus -or -not $Dateien) {
            throw 'Either -NeueCodes <Anzahl>, or -Tag, -Liste, -Aus and -Dateien.'
        }
        $full = (Resolve-Path $Liste).Path
        if ($full.StartsWith($repo, [StringComparison]::OrdinalIgnoreCase)) {
            throw 'The list of testers must lie outside the repository.'
        }
        $arguments = @('verschluesseln', '--tag', $Tag, '--liste', $full, '--aus', $Aus)
        if ($Schnell) { $arguments += '--schnell' }
        & cargo @cargo @arguments @Dateien
    }
    if ($LASTEXITCODE -ne 0) { throw "beta_verschluesseln failed ($LASTEXITCODE)" }
}
finally {
    Pop-Location
}
