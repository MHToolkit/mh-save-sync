#!/usr/bin/env bash
set -euo pipefail

if [[ $# -gt 1 ]]; then
  echo "usage: $0 [output-directory]" >&2
  exit 2
fi

if [[ "$(uname -m)" != "arm64" ]]; then
  echo "mh3g macOS package requires an arm64 Apple Silicon host" >&2
  exit 1
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output_dir="${1:-$repo_root/artifacts}"
mkdir -p "$output_dir"
output_dir="$(cd "$output_dir" && pwd)"

stage_name="mh3g-save-convert-macos-arm64"
stage="$output_dir/$stage_name"
archive="$output_dir/$stage_name.zip"
archive_checksum="$archive.sha256"
verify_dir="$output_dir/.${stage_name}-verify"
binary="$stage/mh3g-save-convert"

rm -rf "$stage" "$verify_dir"
rm -f "$archive" "$archive_checksum"

cd "$repo_root"
cargo build --locked --release -p mh3g-save-convert --bin mh3g-save-convert

mkdir -p "$stage"
install -m 0755 target/release/mh3g-save-convert "$binary"
cp packaging/mh3g-save-convert/README-macOS.txt "$stage/README-macOS.txt"

if command -v codesign >/dev/null 2>&1; then
  codesign --force --sign - "$binary"
  codesign --verify "$binary"
fi

"$binary" --help >/dev/null
(
  cd "$stage"
  shasum -a 256 mh3g-save-convert > mh3g-save-convert.sha256
)

ditto -c -k --keepParent "$stage" "$archive"
(
  cd "$output_dir"
  shasum -a 256 "$(basename "$archive")" > "$(basename "$archive_checksum")"
)

mkdir -p "$verify_dir"
ditto -x -k "$archive" "$verify_dir"
(
  cd "$verify_dir/$stage_name"
  shasum -a 256 -c mh3g-save-convert.sha256
  ./mh3g-save-convert --help >/dev/null
)
rm -rf "$verify_dir"

printf 'package: %s\nchecksum: %s\n' "$archive" "$archive_checksum"
