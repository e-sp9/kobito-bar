//! キーマップ画像の取得と配信(マイルストーン4、v1 = 静的表示)。
//!
//! KobitoKey_QWERTY リポジトリの `images/layer0-3.png` は
//! `scripts/generate_keymap_images.py` が実キーマップから自動生成しており、
//! GitHub raw を参照すればキーマップ更新に追従できる。方針:
//!
//! 1. 表示はローカル最速: キャッシュ(app_data_dir)→ 同梱画像の順で即返す
//! 2. 裏で GitHub raw から更新し、変化があれば `keymap-updated` で配信する
//!    (stale-while-revalidate)。オフラインなら 1. のまま
//!
//! v2(将来)は ZMK Studio RPC での動的取得に置き換える。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use base64::Engine;
use serde::Serialize;
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// レイヤーの表示名。KobitoKey.keymap の label と対応
/// (Layer 0 はラベルなしのデフォルトレイヤーなので QWERTY と表記)
const LAYER_NAMES: [&str; 4] = ["QWERTY", "NUMBER", "SYMBOL", "MOUSE"];

const RAW_BASE_URL: &str = "https://raw.githubusercontent.com/e-sp9/KobitoKey_QWERTY/main/images";

const FETCH_TIMEOUT: Duration = Duration::from_secs(8);

/// フロントエンドとの IPC 契約。`get_keymap_images` コマンドの戻り値であり、
/// `keymap-updated` イベントのペイロードでもある
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeymapImage {
    pub layer: usize,
    pub name: &'static str,
    /// `<img src>` にそのまま使える data URL(data:image/png;base64,…)
    pub data_url: String,
    /// 画像の出どころ。"network" = 今セッションで取得した最新 /
    /// "cache" = 過去に取得したもの / "bundled" = アプリ同梱(オフライン初回)
    pub source: &'static str,
}

/// バックグラウンド更新の制御。lib.rs の setup で manage される
#[derive(Default)]
pub struct KeymapState {
    /// 更新タスクの多重起動防止
    refreshing: AtomicBool,
    /// このセッションでネットワーク更新に成功済みか(成功後は再試行しない)
    refreshed: AtomicBool,
}

/// キーマップ画像を返す。ローカル(キャッシュ → 同梱)を即返し、必要なら
/// バックグラウンドの更新タスクを起動する
#[tauri::command]
pub async fn get_keymap_images(app: AppHandle) -> Result<Vec<KeymapImage>, String> {
    let images = load_local(&app)?;
    spawn_refresh_if_needed(&app);
    Ok(images)
}

/// キーマップウィンドウを表示する(ポップアップ内のボタンから)
#[tauri::command]
pub fn show_keymap_window(app: AppHandle) {
    show(&app);
}

/// トレイメニューとコマンドの両方から使う表示処理
pub fn show<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("keymap") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// キャッシュ → 同梱の順に読んで全レイヤーを組み立てる
fn load_local(app: &AppHandle) -> Result<Vec<KeymapImage>, String> {
    let cache = cache_dir(app);
    LAYER_NAMES
        .iter()
        .enumerate()
        .map(|(layer, name)| {
            let (bytes, source) = match std::fs::read(cache.join(file_name(layer))) {
                Ok(bytes) => (bytes, "cache"),
                Err(_) => (read_bundled(app, layer)?, "bundled"),
            };
            Ok(KeymapImage {
                layer,
                name,
                data_url: to_data_url(&bytes),
                source,
            })
        })
        .collect()
}

fn read_bundled(app: &AppHandle, layer: usize) -> Result<Vec<u8>, String> {
    let path = app
        .path()
        .resolve(
            format!("keymaps/{}", file_name(layer)),
            BaseDirectory::Resource,
        )
        .map_err(|e| e.to_string())?;
    std::fs::read(&path)
        .map_err(|e| format!("同梱キーマップ画像を読めません({}): {e}", path.display()))
}

/// 未更新のセッションなら GitHub raw からの更新タスクを起動する。
/// 成否にかかわらず表示は load_local の結果で継続する
fn spawn_refresh_if_needed(app: &AppHandle) {
    let state = app.state::<KeymapState>();
    if state.refreshed.load(Ordering::Relaxed) {
        return;
    }
    if state.refreshing.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = refresh_cache(&app).await;
        let state = app.state::<KeymapState>();
        state.refreshing.store(false, Ordering::SeqCst);
        match result {
            Ok(changed) => {
                state.refreshed.store(true, Ordering::Relaxed);
                eprintln!("[keymap] キーマップ画像を更新(変化: {changed})");
                if changed {
                    if let Ok(images) = load_local(&app) {
                        let _ = app.emit("keymap-updated", &images);
                    }
                }
            }
            // オフライン等では正常系。キャッシュ/同梱画像の表示のまま次回再試行
            Err(e) => eprintln!("[keymap] 更新できませんでした(表示は継続): {e}"),
        }
    });
}

/// 全レイヤーを GitHub raw から取得してキャッシュへ保存する。
/// 戻り値はキャッシュ内容に変化があったかどうか
async fn refresh_cache(app: &AppHandle) -> Result<bool, String> {
    let client = reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?;
    let cache = cache_dir(app);
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;

    let mut changed = false;
    for layer in 0..LAYER_NAMES.len() {
        let url = format!("{RAW_BASE_URL}/{}", file_name(layer));
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
        // captive portal のリダイレクト先 HTML などをキャッシュしないための保険
        if !is_png(&bytes) {
            return Err(format!("{url} の応答が PNG ではありません"));
        }
        let path = cache.join(file_name(layer));
        if std::fs::read(&path).ok().as_deref() != Some(bytes.as_ref()) {
            std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
            changed = true;
        }
    }
    Ok(changed)
}

fn cache_dir(app: &AppHandle) -> PathBuf {
    // app_data_dir が取れない環境では実在しないパスになり、読み書きが
    // 失敗して同梱画像へフォールバックするだけ(パニックさせない)
    app.path()
        .app_data_dir()
        .map(|d| d.join("keymaps"))
        .unwrap_or_else(|_| PathBuf::from("kobito-bar-cache-unavailable/keymaps"))
}

fn file_name(layer: usize) -> String {
    format!("layer{layer}.png")
}

fn to_data_url(bytes: &[u8]) -> String {
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x89, b'P', b'N', b'G'])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_magic_detection() {
        assert!(is_png(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]));
        assert!(!is_png(b"<!DOCTYPE html>"));
        assert!(!is_png(b""));
    }

    #[test]
    fn data_url_has_png_header() {
        let url = to_data_url(&[1, 2, 3]);
        assert!(url.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn layer_file_names() {
        assert_eq!(file_name(0), "layer0.png");
        assert_eq!(file_name(3), "layer3.png");
    }
}
