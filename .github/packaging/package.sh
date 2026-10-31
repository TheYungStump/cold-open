#!/usr/bin/env bash
# Packages release builds of the app and the CLI (docs/specs/release.md):
#
#   package.sh windows-stage <x64|arm64> <target>   the files, in dist/stage
#   package.sh windows-pack  <x64|arm64>            the zip and the installer
#   package.sh macos                                 the universal .dmg (or two)
#   package.sh linux <x86_64|aarch64> <target>       .deb, AppImage, Flatpak
#   package.sh sizes                                 checks dist/ (15,000,000 bytes)
#
# Windows is two steps so the exes can be signed between them, and the
# installer after (release.md §8). COLDOPEN_THIRD_PARTY names the
# third-party license file the build compiled in (release.md §10).
# Everything goes to dist/, and the sizes to $GITHUB_STEP_SUMMARY when set.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
cd "$root"
version=$(cargo pkgid -p coldopen-app | sed 's/.*[#@]//')
commit=$(git rev-parse HEAD 2>/dev/null | cut -c1-12 || echo unknown)
id=io.github.theyungstump.coldopen
# The budget for every file a writer downloads (release.md §7).
limit=15000000

licenses() {
  local dir=$1
  mkdir -p "$dir"
  {
    echo "Cold Open $version is free software, licensed under the GNU General Public"
    echo "License, version 3 or (at your option) any later version. Its source code:"
    echo "https://github.com/TheYungStump/cold-open/tree/$commit"
    echo
    cat crates/app/LICENSE
  } >"$dir/COPYING.txt"
  cp LICENSE-MIT "$dir/LICENSE-MIT.txt"
  cp LICENSE-APACHE "$dir/LICENSE-APACHE.txt"
  cp crates/fonts/fonts/OFL.txt "$dir/Courier-Prime-OFL.txt"
  cp crates/fonts/fonts/OFL-AtkinsonHyperlegibleNext.txt "$dir/Atkinson-Hyperlegible-Next-OFL.txt"
  if [ -n "${COLDOPEN_THIRD_PARTY:-}" ]; then
    cp "$COLDOPEN_THIRD_PARTY" "$dir/THIRD-PARTY-LICENSES.txt"
  else
    echo "COLDOPEN_THIRD_PARTY is not set: no third-party license file" >&2
    exit 1
  fi
}

readme() {
  sed "s/@VERSION@/$version/" "$here/TESTING.txt" >"$1"
}

windows_stage() {
  local arch=$1 target=$2
  local bin=target/$target/release stage=dist/stage/windows-$arch
  rm -rf "$stage"
  mkdir -p "$stage/bin"
  cp "$bin/coldopen-app.exe" "$stage/Cold Open.exe"
  cp "$bin/coldopen.exe" "$stage/bin/coldopen.exe"
  licenses "$stage/licenses"
  readme "$stage/README.txt"
}

windows_pack() {
  local arch=$1
  local stage=dist/stage/windows-$arch
  # No space in the folder name, so a terminal path to coldopen.exe
  # needs no quoting (the project owner's manual test, 2026-10-31).
  local zipdir=dist/stage/zip-$arch/ColdOpen
  rm -rf "dist/stage/zip-$arch"
  mkdir -p "$zipdir"
  # The zip has the CLI beside the app (release.md §3.2).
  cp "$stage/Cold Open.exe" "$stage/bin/coldopen.exe" "$stage/README.txt" "$zipdir/"
  cp -r "$stage/licenses" "$zipdir/"
  (cd "dist/stage/zip-$arch" && 7z a -tzip -mx=9 "../../ColdOpen-$version-windows-$arch.zip" ColdOpen >/dev/null)
  local iscc="${ISCC:-/c/Program Files (x86)/Inno Setup 6/ISCC.exe}"
  # Git Bash would turn the /D… switches into paths.
  MSYS_NO_PATHCONV=1 MSYS2_ARG_CONV_EXCL='*' "$iscc" /Q "/DAppVersion=$version" "/DNumVersion=${version%%-*}.0" "/DArch=$arch" \
    "/DStage=$(cygpath -w "$root/$stage")" "/DOut=$(cygpath -w "$root/dist")" \
    "$(cygpath -w "$here/windows/coldopen.iss")"
}

macos_bundle() {
  # $1: where, $2...: the binaries' folders to join
  local app=$1
  shift
  mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
  local apps=() clis=()
  for b in "$@"; do
    apps+=("$b/coldopen-app")
    clis+=("$b/coldopen")
  done
  lipo -create "${apps[@]}" -output "$app/Contents/MacOS/coldopen-app"
  lipo -create "${clis[@]}" -output "$app/Contents/MacOS/coldopen"
  sed -e "s/@VERSION@/$version/g" -e "s/@SHORT_VERSION@/${version%%-*}/g" \
    "$here/Info.plist" >"$app/Contents/Info.plist"
  cp "$here/icon/coldopen.icns" "$app/Contents/Resources/coldopen.icns"
  licenses "$app/Contents/Resources/licenses"
  # Ad hoc: unsigned, but whole, so Apple Silicon runs it once allowed
  # (release.md §8). The CLI first, then the bundle around it.
  codesign --force --sign - "$app/Contents/MacOS/coldopen"
  codesign --force --sign - "$app"
  codesign --verify --strict "$app"
}

macos_dmg() {
  # $1: name, $2...: the binaries' folders
  local name=$1
  shift
  local stage=dist/stage/$name
  rm -rf "$stage"
  mkdir -p "$stage"
  macos_bundle "$stage/Cold Open.app" "$@"
  ln -s /Applications "$stage/Applications"
  readme "$stage/Read me first.txt"
  hdiutil create -quiet -volname "Cold Open" -srcfolder "$stage" -fs HFS+ \
    -format ULMO -ov "dist/$name.dmg"
}

macos() {
  local arm=target/aarch64-apple-darwin/release intel=target/x86_64-apple-darwin/release
  macos_dmg "ColdOpen-$version-macos-universal" "$arm" "$intel"
  local bytes
  bytes=$(wc -c <"dist/ColdOpen-$version-macos-universal.dmg" | tr -d ' ')
  if [ "$bytes" -ge "$limit" ]; then
    # One image per architecture instead (release.md §2).
    echo "universal image is $bytes bytes; making one per architecture" >&2
    rm "dist/ColdOpen-$version-macos-universal.dmg"
    macos_dmg "ColdOpen-$version-macos-arm64" "$arm"
    macos_dmg "ColdOpen-$version-macos-x64" "$intel"
  fi
}

linux_tree() {
  # The files every Linux package installs, under $1 (a /usr prefix).
  local usr=$1 bin=$2
  install -Dm755 "$bin/coldopen-app" "$usr/bin/coldopen-app"
  install -Dm755 "$bin/coldopen" "$usr/bin/coldopen"
  install -Dm644 "$here/linux/$id.desktop" "$usr/share/applications/$id.desktop"
  install -Dm644 "$here/linux/$id.xml" "$usr/share/mime/packages/$id.xml"
  mkdir -p "$usr/share/metainfo"
  sed -e "s/@VERSION@/$version/" -e "s/@DATE@/$(date -u +%Y-%m-%d)/" \
    "$here/linux/$id.metainfo.xml" >"$usr/share/metainfo/$id.metainfo.xml"
  for s in 16 24 32 48 64 128 256 512; do
    install -Dm644 "$here/icon/png/$s.png" "$usr/share/icons/hicolor/${s}x$s/apps/$id.png"
  done
}

linux_deb() {
  local arch=$1 bin=$2
  local debarch
  debarch=$([ "$arch" = x86_64 ] && echo amd64 || echo arm64)
  local root_dir=dist/stage/deb-$arch
  rm -rf "$root_dir"
  linux_tree "$root_dir/usr" "$bin"
  licenses "$root_dir/usr/share/doc/cold-open/licenses"
  cat >"$root_dir/usr/share/doc/cold-open/copyright" <<EOF
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: Cold Open
Source: https://github.com/TheYungStump/cold-open

Files: *
Copyright: The Cold Open contributors
License: GPL-3.0-or-later
 On Debian systems the license is in /usr/share/common-licenses/GPL-3.
 Fonts (SIL OFL 1.1) and third-party crates: see licenses/ beside this file.
EOF
  # dpkg-shlibdeps reads a debian/control from the folder it runs in.
  local work=dist/stage/shlibs-$arch
  rm -rf "$work"
  mkdir -p "$work/debian"
  printf 'Source: cold-open\n\nPackage: cold-open\nArchitecture: any\n' >"$work/debian/control"
  local depends
  depends=$(cd "$work" && dpkg-shlibdeps -O "$root/$root_dir/usr/bin/coldopen-app" "$root/$root_dir/usr/bin/coldopen" | sed 's/^shlibs:Depends=//')
  # The window system's libraries, which winit loads at run time, so the
  # linker never names them: X11 and Wayland both.
  depends="$depends, libxkbcommon0, libxkbcommon-x11-0, libx11-xcb1, libxcursor1, libxi6, libxrandr2, libwayland-client0, libwayland-cursor0"
  local size
  size=$(du -sk "$root_dir/usr" | cut -f1)
  mkdir -p "$root_dir/DEBIAN"
  cat >"$root_dir/DEBIAN/control" <<EOF
Package: cold-open
Version: ${version//-/\~}
Architecture: $debarch
Maintainer: The Cold Open contributors <https://github.com/TheYungStump/cold-open/issues>
Installed-Size: $size
Depends: $depends
Recommends: xdg-desktop-portal
Section: editors
Priority: optional
Homepage: https://github.com/TheYungStump/cold-open
Description: screenwriting app for Fountain and .fdx scripts
 Cold Open writes screenplays, paginates them the same on every computer,
 and journals every keystroke, so a crash costs nothing. Every action is a
 command, reachable from the app, the coldopen command, JSON over stdio and
 an MCP server for AI agents.
EOF
  dpkg-deb --root-owner-group -Zxz --build "$root_dir" "dist/cold-open_${version}_$debarch.deb" >/dev/null
}

linux_appimage() {
  local arch=$1 bin=$2
  local appdir=dist/stage/AppDir-$arch
  rm -rf "$appdir"
  linux_tree "$appdir/usr" "$bin"
  licenses "$appdir/usr/share/licenses/cold-open"
  install -Dm755 "$here/linux/AppRun" "$appdir/AppRun"
  cp "$here/linux/$id.desktop" "$appdir/$id.desktop"
  cp "$here/icon/png/256.png" "$appdir/$id.png"
  ln -sf "$id.png" "$appdir/.DirIcon"
  # appimagetool and the runtime it prepends, pinned (release.md §3.5).
  local tool=dist/stage/appimagetool-$arch runtime=dist/stage/runtime-$arch
  local tool_sha runtime_sha
  case "$arch" in
    x86_64)
      tool_sha=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
      runtime_sha=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d
      ;;
    aarch64)
      tool_sha=f0837e7448a0c1e4e650a93bb3e85802546e60654ef287576f46c71c126a9158
      runtime_sha=00cbdfcf917cc6c0ff6d3347d59e0ca1f7f45a6df1a428a0d6d8a78664d87444
      ;;
  esac
  curl -fsSL -o "$tool" "https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-$arch.AppImage"
  curl -fsSL -o "$runtime" "https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-$arch"
  echo "$tool_sha  $tool" | sha256sum -c -
  echo "$runtime_sha  $runtime" | sha256sum -c -
  chmod +x "$tool"
  APPIMAGE_EXTRACT_AND_RUN=1 ARCH=$arch "$tool" --no-appstream --runtime-file "$runtime" \
    "$appdir" "dist/ColdOpen-$version-$arch.AppImage" >/dev/null
}

linux_flatpak() {
  # Flatpak's own build commands, which put the release binaries in /app
  # as they are: no build from source, so no flatpak-builder (whose
  # Ubuntu 22.04 version calls a tool the 25.08 SDK no longer has, and
  # whose Flathub version can't see the runner's SDK from its sandbox).
  local arch=$1 bin=$2
  local work=dist/stage/flatpak-$arch
  rm -rf "$work"
  flatpak build-init "$work/build" "$id" org.freedesktop.Sdk org.freedesktop.Platform 25.08
  # The build folder's files/ is /app inside the sandbox.
  linux_tree "$work/build/files" "$bin"
  licenses "$work/build/files/share/licenses/$id"
  # The permissions (release.md §3.6).
  flatpak build-finish "$work/build" --command=coldopen-app \
    --share=ipc --socket=wayland --socket=fallback-x11 --device=dri \
    --filesystem=home --filesystem=xdg-run/coldopen:create >/dev/null
  flatpak build-export "$work/repo" "$work/build" >/dev/null
  flatpak build-bundle --runtime-repo=https://flathub.org/repo/flathub.flatpakrepo \
    "$work/repo" "dist/ColdOpen-$version-$arch.flatpak" "$id"
}

linux() {
  local arch=$1 target=$2
  local bin=target/$target/release
  linux_deb "$arch" "$bin"
  linux_appimage "$arch" "$bin"
  linux_flatpak "$arch" "$bin"
}

sizes() {
  local over=0 mib
  {
    echo "### Packages"
    echo
    echo "| File | Bytes | Budget |"
    echo "| --- | --- | --- |"
  } >dist/sizes.md
  for f in dist/*.exe dist/*.zip dist/*.dmg dist/*.deb dist/*.AppImage dist/*.flatpak; do
    [ -e "$f" ] || continue
    local b
    b=$(wc -c <"$f" | tr -d ' ')
    mib=$(awk -v b="$b" 'BEGIN { printf "%.2f MiB", b / 1048576 }')
    if [ "$b" -ge "$limit" ]; then
      echo "| $(basename "$f") | $b ($mib) | OVER 15,000,000 |" >>dist/sizes.md
      echo "::error::$(basename "$f") is $b bytes, over the 15,000,000-byte budget"
      over=1
    else
      echo "| $(basename "$f") | $b ($mib) | under 15,000,000 |" >>dist/sizes.md
    fi
  done
  cat dist/sizes.md
  if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then cat dist/sizes.md >>"$GITHUB_STEP_SUMMARY"; fi
  rm dist/sizes.md
  return "$over"
}

mkdir -p dist
case "${1:-}" in
  windows-stage) windows_stage "$2" "$3" ;;
  windows-pack) windows_pack "$2" ;;
  macos) macos ;;
  linux) linux "$2" "$3" ;;
  sizes) sizes ;;
  *)
    sed -n '2,15p' "$0" >&2
    exit 2
    ;;
esac
