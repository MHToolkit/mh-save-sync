#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
real_home="${HOME}"
export RUSTUP_HOME="${RUSTUP_HOME:-$real_home/.rustup}"
export CARGO_HOME="${CARGO_HOME:-$real_home/.cargo}"
mkdir -p "$tmp/home" "$tmp/Applications"

if HOME="$tmp/home" MH_SAVE_SYNC_INSTALL_DIR="/" ./scripts/install-macos-app.sh >/dev/null 2>&1; then
  echo "install script accepted unsafe install dir" >&2
  exit 1
fi
if HOME="$tmp/home" MH_SAVE_SYNC_INSTALL_DIR="$tmp/Applications" MH_SAVE_SYNC_APP_NAME="bad/name.app" ./scripts/install-macos-app.sh >/dev/null 2>&1; then
  echo "install script accepted app name with slash" >&2
  exit 1
fi
if HOME="$tmp/home" MH_SAVE_SYNC_INSTALL_DIR="$tmp/Applications" MH_SAVE_SYNC_APP_NAME="bad" ./scripts/install-macos-app.sh >/dev/null 2>&1; then
  echo "install script accepted non-.app name" >&2
  exit 1
fi

output="$(HOME="$tmp/home" MH_SAVE_SYNC_INSTALL_DIR="$tmp/Applications" ./scripts/install-macos-app.sh)"
app="$tmp/Applications/MH Save Sync.app"
exe="$app/Contents/MacOS/MHSaveSyncMac"
cli="$app/Contents/MacOS/mh-save"
test -x "$exe"
test -x "$cli"
grep -q '"macos_app_installed": true' <<<"$output"

mkdir -p "$tmp/home/Documents/Secrets" "$tmp/save-root/slot1"
printf "6666666666666666666666666666666666666666666666666666666666666666" > "$tmp/home/Documents/Secrets/mh-save-sync-test-secret.hex"

HOME="$tmp/home" "$exe" --set-server-url http://127.0.0.1:39082 >/dev/null
HOME="$tmp/home" "$exe" --set-save-root "$tmp/save-root" >/dev/null
HOME="$tmp/home" "$exe" --set-recovery-secret-file "$tmp/home/Documents/Secrets/mh-save-sync-test-secret.hex" >/dev/null
HOME="$tmp/home" "$exe" --auto-upload-on-exit off >/dev/null
status="$(HOME="$tmp/home" "$exe" --status)"
grep -q "同步到服务器：http://127.0.0.1:39082" <<<"$status"
grep -q "下一步：启动 MH3G 前点「启动前检查」" <<<"$status"
grep -q "当前同步对象：MH3G / macOS Nemessix" <<<"$status"
grep -q "Mac 存档目录：$tmp/save-root" <<<"$status"
grep -q "恢复密钥文件：$tmp/home/Documents/Secrets/mh-save-sync-test-secret.hex" <<<"$status"
grep -q "自动同步：已关闭：只手动同步" <<<"$status"
preview="$(HOME="$tmp/home" "$exe" --menu-preview)"
grep -q "菜单栏预览：MH 云存档 · 就绪" <<<"$preview"
grep -q "同步路线：MH3G / macOS Nemessix → 本机安全缓存 → http://127.0.0.1:39082" <<<"$preview"
grep -q "同步存档…" <<<"$preview"
grep -q "上传本地存档" <<<"$preview"
grep -q "用云端恢复本地…" <<<"$preview"
grep -q "处理冲突…" <<<"$preview"
grep -q "云端状态" <<<"$preview"
grep -q "自动同步：已关闭：只手动同步" <<<"$preview"

help="$(HOME="$tmp/home" "$exe" --help)"
grep -q "一级菜单只保留" <<<"$help"
grep -q "同步存档" <<<"$help"
grep -q -- "--set-server-url" <<<"$help"
grep -q "上传本地存档" <<<"$help"
grep -q "云端状态" <<<"$help"
grep -q "处理冲突" <<<"$help"
grep -q "服务器、存档目录和恢复密钥放在「设置」" <<<"$help"
grep -q "历史版本尚未接入此界面" <<<"$help"
"$cli" --help >/dev/null

python3 - <<PY
import json
print(json.dumps({
    "macos_install_e2e": True,
    "install_dir": "$tmp/Applications",
    "installed_app": "$app",
    "server_url_persisted": "http://127.0.0.1:39082",
    "bundled_cli": True,
    "save_root_configured": True,
    "recovery_secret_file_configured": True,
    "auto_upload_on_exit": False,
}, ensure_ascii=False, sort_keys=True))
PY
