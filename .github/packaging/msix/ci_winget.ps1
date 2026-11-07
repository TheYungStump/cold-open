# CI (ci.yml, store-winget-msix): the winget manifests, filled from this
# run's x64 installer (the other files stand in), then `winget validate`.
$ErrorActionPreference = "Stop"
$v = (Select-String -Path Cargo.toml -Pattern '^version = "(.*)"').Matches[0].Groups[1].Value
New-Item -ItemType Directory -Force fake | Out-Null
Copy-Item "dist/ColdOpen-$v-windows-x64-setup.exe" fake/
foreach ($n in "windows-arm64-setup.exe", "macos-universal.dmg", "third-party-licenses-linux-x86_64.txt", "third-party-licenses-linux-aarch64.txt", "mcp.mcpb") {
  "stand-in" | Set-Content "fake/ColdOpen-$v-$n"
}
python .github/packaging/stores.py $v ci 2026-01-01 fake stores
if ($LASTEXITCODE) { throw "stores.py failed" }
if (Get-Command winget -ErrorAction SilentlyContinue) {
  winget validate --manifest "stores/winget/manifests/t/TheYungStump/ColdOpen/$v"
  if ($LASTEXITCODE) { throw "winget validate failed ($LASTEXITCODE)" }
} else {
  Write-Output "::warning::winget isn't on this runner, so the manifests weren't validated"
}
