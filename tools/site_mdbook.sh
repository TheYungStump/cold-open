#!/usr/bin/env bash
# Installs the pinned mdBook for the website's builds (docs/specs/site.md
# §2): the release binary from the mdBook project, checked against its
# SHA-256 before it runs. CI only (Linux x86-64).
set -euo pipefail

version=0.5.4
sha256=3f28de05dafca9d0f2eab99c662116b0e37b89b1d96a08f8f430b9eeae958cd7
name=mdbook-v$version-x86_64-unknown-linux-gnu.tar.gz

dir=$(mktemp -d)
curl -fsSL -o "$dir/$name" "https://github.com/rust-lang/mdBook/releases/download/v$version/$name"
echo "$sha256  $dir/$name" | sha256sum -c -
tar -xzf "$dir/$name" -C "$dir"
mkdir -p "$HOME/.local/bin"
install -m 0755 "$dir/mdbook" "$HOME/.local/bin/mdbook"
echo "$HOME/.local/bin" >>"${GITHUB_PATH:-/dev/null}"
"$HOME/.local/bin/mdbook" --version
