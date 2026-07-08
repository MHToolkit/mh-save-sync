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

help="$(HOME="$tmp/home" "$exe" --help)"
grep -q "双击" <<<"$help"
grep -q "菜单顶部会显示「下一步」" <<<"$help"
grep -q -- "--set-server-url" <<<"$help"
grep -q "立即上传 Mac 存档到服务器" <<<"$help"
grep -q "查看云端状态" <<<"$help"
grep -q "我已退出 MH3G：立即对账上传" <<<"$help"
grep -q "选择 Mac Nemessix 存档目录" <<<"$help"
grep -q "选择恢复密钥文件" <<<"$help"
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
