$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

foreach ($tool in @("python", "clang", "lld-link")) {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
        throw "缺少工具: $tool"
    }
}

python gen_wrapper.py
Remove-Item -Recurse -Force build, dist -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force build, dist | Out-Null

lld-link /lib /def:kernel32.def /machine:x64 /out:build/kernel32.lib
lld-link /lib /def:shell32.def /machine:x64 /out:build/shell32.lib
clang --target=x86_64-pc-windows-msvc -O2 -fno-builtin -ffreestanding `
    -c wrapper.c -o build/wrapper.obj
lld-link /entry:wmainCRTStartup /subsystem:console /machine:x64 /nodefaultlib `
    /out:dist/mh3g-save-convert.exe `
    build/wrapper.obj build/kernel32.lib build/shell32.lib

python validate_patch.py dist/mh3g-save-convert.exe
Get-FileHash dist/mh3g-save-convert.exe -Algorithm SHA256
