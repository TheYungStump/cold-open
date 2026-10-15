#!/usr/bin/env bash
# Packages a release build of the app and the CLI for testers:
#   .github/packaging/package.sh <name> <target>
# Writes dist/cold-open-<name>.zip (Windows, macOS) or .tar.gz (Linux), and
# appends the sizes to $GITHUB_STEP_SUMMARY when it is set.
set -euo pipefail

name=$1
target=$2
bin=target/$target/release
here=$(cd "$(dirname "$0")" && pwd)
stage=dist/stage/cold-open-$name
rm -rf dist
mkdir -p "$stage/licenses"

cp "$here/TESTING.txt" "$stage/"
cp crates/app/LICENSE "$stage/licenses/Cold-Open-GPL-3.0.txt"
cp crates/fonts/fonts/OFL.txt "$stage/licenses/Courier-Prime-OFL.txt"
cp crates/fonts/fonts/OFL-AtkinsonHyperlegibleNext.txt "$stage/licenses/Atkinson-Hyperlegible-Next-OFL.txt"

size() { wc -c <"$1" | tr -d ' '; }

case "$target" in
  *windows*)
    cp "$bin/coldopen-app.exe" "$stage/Cold Open.exe"
    cp "$bin/coldopen.exe" "$stage/"
    app_bytes=$(size "$bin/coldopen-app.exe")
    cli_bytes=$(size "$bin/coldopen.exe")
    archive=dist/cold-open-$name.zip
    (cd dist/stage && 7z a -tzip -mx=9 "../cold-open-$name.zip" "cold-open-$name" >/dev/null)
    ;;
  *apple*)
    app="$stage/Cold Open.app"
    mkdir -p "$app/Contents/MacOS"
    cp "$bin/coldopen-app" "$app/Contents/MacOS/coldopen-app"
    version=$(cargo pkgid -p coldopen-app | sed 's/.*[#@]//')
    sed "s/@VERSION@/$version/g" "$here/Info.plist" >"$app/Contents/Info.plist"
    # Ad-hoc signature: unsigned, but whole, so Apple Silicon will run it
    # once the tester allows it.
    codesign --force --deep --sign - "$app"
    cp "$bin/coldopen" "$stage/"
    app_bytes=$(size "$bin/coldopen-app")
    cli_bytes=$(size "$bin/coldopen")
    archive=dist/cold-open-$name.zip
    (cd dist/stage && ditto -c -k --keepParent "cold-open-$name" "../cold-open-$name.zip")
    ;;
  *linux*)
    cp "$bin/coldopen-app" "$bin/coldopen" "$stage/"
    app_bytes=$(size "$bin/coldopen-app")
    cli_bytes=$(size "$bin/coldopen")
    archive=dist/cold-open-$name.tar.gz
    tar -C dist/stage -czf "$archive" "cold-open-$name"
    ;;
  *)
    echo "unknown target $target" >&2
    exit 1
    ;;
esac

archive_bytes=$(size "$archive")
mib() { awk -v b="$1" 'BEGIN { printf "%.2f MiB", b / 1048576 }'; }
echo "app $(mib "$app_bytes"), CLI $(mib "$cli_bytes"), archive $(mib "$archive_bytes")"
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  {
    echo "### Sizes: $name"
    echo
    echo "| File | Size |"
    echo "| --- | --- |"
    echo "| App (both renderers) | $(mib "$app_bytes") |"
    echo "| CLI with MCP server | $(mib "$cli_bytes") |"
    echo "| Download ($(basename "$archive")) | $(mib "$archive_bytes") (budget: under 15 MB) |"
  } >>"$GITHUB_STEP_SUMMARY"
fi
rm -rf dist/stage
