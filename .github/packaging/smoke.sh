#!/usr/bin/env bash
# The checks CI runs on an installed package (docs/specs/release.md):
#
#   smoke.sh cli <coldopen>         --version, then a headless open, edit,
#                                    save and export of a fixture copy
#   smoke.sh app <app...>            the app's first frame, with a script,
#                                    in a fresh home; then the folders made
#   smoke.sh fallback <app...>       the GPU renderer asked for where it
#                                    can't start: the CPU renderer draws
#
# <app...> may be several words (flatpak run <id>); a Flatpak gets the
# environment with --env, since it has its own.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
# The workspace's version: the first one in the root Cargo.toml.
version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)
work=$(mktemp -d)
script="$work/scene.fountain"
fixture=$(ls "$root/tests/fixtures/fountain" | grep '\.fountain$' | head -1)
cp "$root/tests/fixtures/fountain/$fixture" "$script"

# launch K=V... -- command...: runs the command with those variables.
launch() {
  local vars=()
  while [ "$1" != -- ]; do
    vars+=("$1")
    shift
  done
  shift
  if [ "$1" = flatpak ]; then
    local f=(flatpak "$2")
    shift 2
    for v in "${vars[@]}"; do f+=("--env=$v"); done
    "${f[@]}" "$@"
  else
    env "${vars[@]}" "$@"
  fi
}

cli() {
  local out
  out=$("$@" --version)
  echo "$out"
  case "$out" in
    "coldopen $version ("*) ;;
    *)
      echo "::error::coldopen --version says '$out', not $version"
      exit 1
      ;;
  esac
  cat >"$work/edits.json" <<'EOF'
{"command": "block.insert", "params": {"type": "action", "text": "A line the smoke test added."}}
EOF
  "$@" run "$work/edits.json" --doc "$script" --save >/dev/null
  grep -q "A line the smoke test added." "$script"
  "$@" export "$script" "$work/scene.pdf"
  head -c 4 "$work/scene.pdf" | grep -q '%PDF'
  "$@" convert "$script" "$work/scene.fdx"
  grep -q '<FinalDraft' "$work/scene.fdx"
  echo "cli: open, edit, save and export work"
}

app() {
  local home="$work/home"
  rm -rf "$home"
  local out
  out=$(launch COLDOPEN_HOME="$home" COLDOPEN_EXIT_AFTER_FIRST_FRAME=1 -- "$@" "$script")
  echo "$out"
  echo "$out" | grep -q "first frame"
  # A first start makes only the app's own folders (release.md §6).
  local other
  other=$(ls -A "$home" | grep -vx -e config -e data -e run || true)
  if [ -n "$other" ]; then
    echo "::error::a first start made $other outside the app's folders"
    exit 1
  fi
  echo "app: first frame drawn"
}

fallback() {
  # Settings ask for the GPU; with no Vulkan driver it can't start.
  local home="$work/fallback"
  rm -rf "$home"
  mkdir -p "$home/config"
  echo '{"renderer": "gpu"}' >"$home/config/settings.json"
  local out
  out=$(launch COLDOPEN_HOME="$home" COLDOPEN_BENCH=typing COLDOPEN_BENCH_FRAMES=10 \
    WGPU_BACKEND=vulkan VK_ICD_FILENAMES=/nonexistent.json -- "$@" "$script")
  echo "$out"
  if ! echo "$out" | grep -q '"backend":"tiny-skia"'; then
    echo "::error::the app did not fall back to the CPU renderer"
    exit 1
  fi
  echo "fallback: the CPU renderer drew when the GPU couldn't start"
}

case "${1:-}" in
  cli | app | fallback)
    what=$1
    shift
    "$what" "$@"
    ;;
  *)
    sed -n '2,11p' "$0" >&2
    exit 2
    ;;
esac
rm -rf "$work"
