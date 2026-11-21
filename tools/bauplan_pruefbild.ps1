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

# Makes a test picture for the blueprint scan from a screenshot of the crafting terminal: the picture as raw BGRA
# (`<name>_<width>x<height>.bgra`) in the folder of the test pictures, and its line in `pruefbilder.json` there.
# The screenshot itself is only read.
#
#   pwsh tools/bauplan_pruefbild.ps1 -Bild <screenshot.jpg> -Ordner <folder> -X <pointer x> -Y <pointer y> `
#        -Erwartet "Sedulity"       # or -Keiner when the picture shows no hint
#
# Run on Windows (System.Drawing).

param(
    [Parameter(Mandatory)] [string] $Bild,
    [Parameter(Mandatory)] [string] $Ordner,
    [Parameter(Mandatory)] [int] $X,
    [Parameter(Mandatory)] [int] $Y,
    [string] $Erwartet,
    [switch] $Keiner
)
$ErrorActionPreference = 'Stop'
if (-not $Keiner -and -not $Erwartet) { throw 'Either -Erwartet <catalog name> or -Keiner.' }
Add-Type -AssemblyName System.Drawing
New-Item -ItemType Directory -Force $Ordner | Out-Null

$source = [System.Drawing.Bitmap]::new($Bild)
try {
    $width = $source.Width
    $height = $source.Height
    $rect = [System.Drawing.Rectangle]::new(0, 0, $width, $height)
    $data = $source.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::ReadOnly,
        [System.Drawing.Imaging.PixelFormat]::Format32bppRgb)
    try {
        $raw = [byte[]]::new($width * $height * 4)
        for ($row = 0; $row -lt $height; $row++) {
            $from = [IntPtr]::Add($data.Scan0, $row * $data.Stride)
            [System.Runtime.InteropServices.Marshal]::Copy($from, $raw, $row * $width * 4, $width * 4)
        }
    } finally { $source.UnlockBits($data) }
} finally { $source.Dispose() }

$stem = [System.IO.Path]::GetFileNameWithoutExtension($Bild) -replace '[^A-Za-z0-9-]', '-'
$file = "${stem}_${width}x${height}.bgra"
[System.IO.File]::WriteAllBytes((Join-Path $Ordner $file), $raw)

$listFile = Join-Path $Ordner 'pruefbilder.json'
$list = @()
if (Test-Path $listFile) { $list = @(Get-Content $listFile -Raw -Encoding utf8 | ConvertFrom-Json) }
$list = @($list | Where-Object { $_.datei -ne $file })
$list += [pscustomobject]@{ datei = $file; zeiger = @($X, $Y); erwartet = $(if ($Keiner) { $null } else { $Erwartet }) }
ConvertTo-Json -InputObject $list -Depth 4 | Set-Content $listFile -Encoding utf8
"$file ($width x $height), pointer $X,$Y, expected $(if ($Keiner) { 'none' } else { $Erwartet })"
