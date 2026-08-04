#!/usr/bin/env bash
# WSL から Windows 用 .exe をクロスビルドし、Windows 側で起動する。
# BLE・システムトレイ・WebView2 込みの実機動作確認用
# (WSLg では BLE とトレイが確認できないため)。
#
# 前提: rustup target add x86_64-pc-windows-msvc && cargo install cargo-xwin
set -euo pipefail

cd "$(dirname "$0")/.."

pnpm tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc --no-bundle

WINTMP=$(cd /mnt/c && cmd.exe /c 'echo %TEMP%' 2>/dev/null | tr -d '\r')
DEST=$(wslpath "$WINTMP")/KobitoBar
mkdir -p "$DEST"

# Windows は実行中の exe を上書きできないため、旧プロセスを先に終了させる
(cd /mnt/c && taskkill.exe /IM kobito-bar.exe /F >/dev/null 2>&1) || true
sleep 1

cp src-tauri/target/x86_64-pc-windows-msvc/release/kobito-bar.exe "$DEST/"
# 同梱リソース(キーマップ画像)。Windows の resource_dir は exe と同じディレクトリ
if [ -d src-tauri/target/x86_64-pc-windows-msvc/release/keymaps ]; then
  cp -r src-tauri/target/x86_64-pc-windows-msvc/release/keymaps "$DEST/"
fi
echo "起動: ${WINTMP}\\KobitoBar\\kobito-bar.exe"
echo "ログ: ${WINTMP}\\KobitoBar\\kobito-bar.log"
cd "$DEST"
# GUI サブシステムでも WSL interop 経由なら eprintln が標準ハンドルに届くため、
# BLE 層のログ([ble] ...)をファイルに残す
nohup ./kobito-bar.exe >kobito-bar.log 2>&1 &
