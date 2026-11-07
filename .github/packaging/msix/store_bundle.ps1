# The release's Microsoft Store upload (release.yml, msix): an MSIX for x64
# and for ARM64 from the release's zips, with the Partner Center identity
# in MSIX_IDENTITY_NAME, MSIX_PUBLISHER and MSIX_PUBLISHER_DISPLAY, joined
# in ColdOpen-<version>.msixbundle in store/ (docs/specs/release.md §11.4).
$ErrorActionPreference = "Stop"
$v = $env:VERSION
foreach ($name in "MSIX_IDENTITY_NAME", "MSIX_PUBLISHER", "MSIX_PUBLISHER_DISPLAY") {
  if (-not [Environment]::GetEnvironmentVariable($name)) { throw "store_bundle.ps1: $name isn't set" }
}
New-Item -ItemType Directory -Force store, packages | Out-Null
foreach ($arch in "x64", "arm64") {
  Expand-Archive "dist/ColdOpen-$v-windows-$arch.zip" "stage-$arch"
  $root = (Get-ChildItem "stage-$arch" -Directory | Select-Object -First 1).FullName
  & "$PSScriptRoot/build_msix.ps1" -Arch $arch -Version $v -Stage $root -Out "msix-$arch" `
    -IdentityName $env:MSIX_IDENTITY_NAME -Publisher $env:MSIX_PUBLISHER -PublisherDisplay $env:MSIX_PUBLISHER_DISPLAY
  Copy-Item "msix-$arch/ColdOpen-$v-windows-$arch.msix" packages/
}
# The bundle's version, as build_msix.ps1 makes the packages'.
if ($v -match '^(\d+)\.(\d+)\.(\d+)-[a-z]+\.(\d+)$') {
  $v4 = "{0}.{1}.{2}.0" -f $Matches[1], $Matches[2], ([int]$Matches[3] * 1000 + [int]$Matches[4])
} elseif ($v -match '^(\d+)\.(\d+)\.(\d+)$') {
  $v4 = "{0}.{1}.{2}.0" -f $Matches[1], $Matches[2], ([int]$Matches[3] * 1000 + 999)
} else { throw "store_bundle.ps1: can't make a package version of $v" }
$sdk = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Directory |
  Where-Object { Test-Path (Join-Path $_.FullName "x64\makeappx.exe") } |
  Sort-Object { [version]$_.Name } | Select-Object -Last 1
& (Join-Path $sdk.FullName "x64\makeappx.exe") bundle /o /d packages /bv $v4 /p "store/ColdOpen-$v.msixbundle"
if ($LASTEXITCODE) { throw "makeappx bundle failed ($LASTEXITCODE)" }
Copy-Item packages/*.msix store/
Get-ChildItem store | ForEach-Object { "{0}  {1:N0} bytes" -f $_.Name, $_.Length }
