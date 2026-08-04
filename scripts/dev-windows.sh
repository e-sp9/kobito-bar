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
echo "起動: ${WINTMP}\\KobitoBar\\kobito-bar.exe"
cd "$DEST"
nohup ./kobito-bar.exe >/dev/null 2>&1 &
