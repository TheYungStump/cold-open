#!/usr/bin/env bash
# Installs the disk image in dist/ as a writer would, into ~/Applications,
# and starts both programs (docs/specs/release.md §3.3). Runs on Apple
# Silicon and on Intel.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
arch=$(uname -m)
dmg=$(ls dist/*-macos-universal.dmg 2>/dev/null || true)
if [ -z "$dmg" ]; then
  case "$arch" in
    arm64) dmg=$(ls dist/*-macos-arm64.dmg) ;;
    *) dmg=$(ls dist/*-macos-x64.dmg) ;;
  esac
fi
echo "installing $dmg on $arch"

mnt=$(mktemp -d)
hdiutil attach -nobrowse -readonly -mountpoint "$mnt" "$dmg" >/dev/null
[ -L "$mnt/Applications" ] || { echo "::error::no Applications alias in the image"; exit 1; }
[ -f "$mnt/Read me first.txt" ] || { echo "::error::no read-me in the image"; exit 1; }
mkdir -p "$HOME/Applications"
rm -rf "$HOME/Applications/Cold Open.app"
cp -R "$mnt/Cold Open.app" "$HOME/Applications/"
hdiutil detach "$mnt" >/dev/null

app="$HOME/Applications/Cold Open.app"
codesign --verify --strict --verbose=1 "$app"
plutil -lint "$app/Contents/Info.plist"
want=$([ "$arch" = arm64 ] && echo arm64 || echo x86_64)
for b in coldopen-app coldopen; do
  archs=$(lipo -archs "$app/Contents/MacOS/$b")
  echo "$b: $archs"
  case " $archs " in
    *" $want "*) ;;
    *) echo "::error::$b has no $want code"; exit 1 ;;
  esac
done
[ -f "$app/Contents/Resources/licenses/THIRD-PARTY-LICENSES.txt" ]
[ -f "$app/Contents/Resources/coldopen.icns" ]

bash "$here/smoke.sh" cli "$app/Contents/MacOS/coldopen"
bash "$here/smoke.sh" app "$app/Contents/MacOS/coldopen-app"
bash "$here/smoke.sh" fallback "$app/Contents/MacOS/coldopen-app"
