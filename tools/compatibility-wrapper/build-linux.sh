#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$HERE"

for tool in python3 clang lld-link; do
  command -v "$tool" >/dev/null 2>&1 || { echo "缺少工具: $tool" >&2; exit 1; }
done

python3 gen_wrapper.py
rm -rf build dist
mkdir -p build dist

lld-link /lib /def:kernel32.def /machine:x64 /out:build/kernel32.lib
lld-link /lib /def:shell32.def /machine:x64 /out:build/shell32.lib
clang --target=x86_64-pc-windows-msvc -O2 -fno-builtin -ffreestanding   -c wrapper.c -o build/wrapper.obj
lld-link /entry:wmainCRTStartup /subsystem:console /machine:x64 /nodefaultlib   /out:dist/mh3g-save-convert.exe   build/wrapper.obj build/kernel32.lib build/shell32.lib

python3 validate_patch.py dist/mh3g-save-convert.exe
sha256sum dist/mh3g-save-convert.exe 2>/dev/null || true
