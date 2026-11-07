# CI (ci.yml, store-winget-msix): the MSIX packed from this run's x64 zip,
# under the 15 MB budget; signed with a throwaway certificate, installed,
# started from Start's entry, and its CLI run through the app execution
# alias; then removed (docs/specs/release.md §11.4).
$ErrorActionPreference = "Stop"
$v = (Select-String -Path Cargo.toml -Pattern '^version = "(.*)"').Matches[0].Groups[1].Value

Expand-Archive "dist/ColdOpen-$v-windows-x64.zip" stage
$root = (Get-ChildItem stage -Directory | Select-Object -First 1).FullName
& "$PSScriptRoot/build_msix.ps1" -Arch x64 -Version $v -Stage $root -Out msix
$msix = "msix/ColdOpen-$v-windows-x64.msix"
$size = (Get-Item $msix).Length
if ($size -ge 15000000) { throw "the MSIX is $size bytes, over 15,000,000" }

# A throwaway certificate whose subject is the test identity's publisher.
$cert = New-SelfSignedCertificate -Type Custom -Subject "CN=ColdOpenTest" -KeyUsage DigitalSignature `
  -FriendlyName "Cold Open CI" -CertStoreLocation Cert:\CurrentUser\My `
  -TextExtension @("2.5.29.37={text}1.3.6.1.5.5.7.3.3", "2.5.29.19={text}")
$pw = ConvertTo-SecureString -String "ci" -Force -AsPlainText
Export-PfxCertificate -Cert $cert -FilePath ci.pfx -Password $pw | Out-Null
Import-PfxCertificate -FilePath ci.pfx -CertStoreLocation Cert:\LocalMachine\TrustedPeople -Password $pw | Out-Null
$sdk = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Directory |
  Where-Object { Test-Path (Join-Path $_.FullName "x64\signtool.exe") } |
  Sort-Object { [version]$_.Name } | Select-Object -Last 1
& (Join-Path $sdk.FullName "x64\signtool.exe") sign /fd SHA256 /f ci.pfx /p ci $msix
if ($LASTEXITCODE) { throw "signtool failed" }

Add-AppxPackage -Path $msix
$pkg = Get-AppxPackage -Name ColdOpen.Test
if (-not $pkg) { throw "the package didn't install" }
Write-Output "installed $($pkg.PackageFullName)"

# The CLI, through its alias, inside the package.
$alias = Join-Path $env:LOCALAPPDATA "Microsoft\WindowsApps\coldopen.exe"
$out = & $alias --version
Write-Output "coldopen --version: $out"
if ("$out" -notmatch [regex]::Escape($v)) { throw "coldopen --version through the alias said: $out" }

# The app, as Start opens it; still running a few seconds later.
Start-Process "shell:AppsFolder\$($pkg.PackageFamilyName)!ColdOpen"
Start-Sleep -Seconds 8
$p = Get-Process | Where-Object { $_.Path -like "*WindowsApps*Cold Open.exe" }
if (-not $p) { throw "the app didn't start from the package" }
Write-Output "running from $($p[0].Path)"
$p | Stop-Process -Force
Remove-AppxPackage $pkg.PackageFullName
