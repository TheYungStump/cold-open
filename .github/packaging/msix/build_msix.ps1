# Packs Cold Open's MSIX for one architecture (docs/specs/release.md §11.4),
# from the files package.sh staged for the zip:
#
#   build_msix.ps1 -Arch x64 -Version 0.1.0-alpha.3 -Stage dist/stage/windows-x64 -Out dist `
#     [-IdentityName Cold.Open.Test] [-Publisher CN=ColdOpenTest] [-PublisherDisplay "The Cold Open contributors"]
#
# The identity defaults to a test one; a Store build passes the values the
# owner's Partner Center reservation gives. MSIX versions are four numbers,
# so a.b.c-alpha.N becomes a.b.(1000c+N).0 and a.b.c becomes a.b.(1000c+999).0:
# each release sorts after the one before, and the last number stays 0, as
# the Store asks.
param(
  [Parameter(Mandatory)] [ValidateSet("x64", "arm64")] [string] $Arch,
  [Parameter(Mandatory)] [string] $Version,
  [Parameter(Mandatory)] [string] $Stage,
  [Parameter(Mandatory)] [string] $Out,
  [string] $IdentityName = "ColdOpen.Test",
  [string] $Publisher = "CN=ColdOpenTest",
  [string] $PublisherDisplay = "The Cold Open contributors"
)
$ErrorActionPreference = "Stop"
$here = $PSScriptRoot

function Version4([string] $v) {
  if ($v -match '^(\d+)\.(\d+)\.(\d+)-[a-z]+\.(\d+)$') {
    return "{0}.{1}.{2}.0" -f $Matches[1], $Matches[2], ([int]$Matches[3] * 1000 + [int]$Matches[4])
  }
  if ($v -match '^(\d+)\.(\d+)\.(\d+)$') {
    return "{0}.{1}.{2}.0" -f $Matches[1], $Matches[2], ([int]$Matches[3] * 1000 + 999)
  }
  throw "build_msix.ps1: can't make a package version of $v"
}

# The newest Windows SDK's makeappx on this machine.
$sdk = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Directory |
  Where-Object { Test-Path (Join-Path $_.FullName "x64\makeappx.exe") } |
  Sort-Object { [version]$_.Name } | Select-Object -Last 1
if (-not $sdk) { throw "build_msix.ps1: no Windows SDK with makeappx.exe" }
$makeappx = Join-Path $sdk.FullName "x64\makeappx.exe"

$v4 = Version4 $Version
$dir = Join-Path $Out "msix-$Arch"
if (Test-Path $dir) { Remove-Item -Recurse -Force $dir }
New-Item -ItemType Directory -Force $dir | Out-Null
Copy-Item -Recurse -Force (Join-Path $Stage "*") $dir
python (Join-Path $here "make_assets.py") (Join-Path $dir "assets")
if ($LASTEXITCODE) { throw "make_assets.py failed" }

# The CLI is in bin\ in the installer's layout and beside the app in the
# zip's; the alias names whichever this stage has.
$cli = if (Test-Path (Join-Path $dir "bin\coldopen.exe")) { "bin\coldopen.exe" }
  elseif (Test-Path (Join-Path $dir "coldopen.exe")) { "coldopen.exe" }
  else { throw "build_msix.ps1: no coldopen.exe in $Stage" }

$manifest = Get-Content -Raw -Encoding utf8 (Join-Path $here "AppxManifest.xml")
$values = @{
  "IDENTITY_NAME" = $IdentityName; "PUBLISHER" = $Publisher; "PUBLISHER_DISPLAY" = $PublisherDisplay
  "VERSION4" = $v4; "ARCH" = $Arch; "CLI" = $cli
}
foreach ($k in $values.Keys) { $manifest = $manifest.Replace("@$k@", [System.Security.SecurityElement]::Escape($values[$k])) }
if ($manifest -match '@[A-Z0-9_]+@') { throw "build_msix.ps1: the manifest still has $($Matches[0])" }
[System.IO.File]::WriteAllText((Join-Path $dir "AppxManifest.xml"), $manifest, (New-Object System.Text.UTF8Encoding $false))

$msix = Join-Path $Out "ColdOpen-$Version-windows-$Arch.msix"
& $makeappx pack /o /d $dir /p $msix
if ($LASTEXITCODE) { throw "makeappx pack failed ($LASTEXITCODE)" }
Write-Output "$msix ($v4, $((Get-Item $msix).Length) bytes)"
