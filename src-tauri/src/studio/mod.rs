//! ZMK Studio RPC による実機キーマップの動的取得(v2)。
//!
//! フロントは従来どおり画像(keymap.rs)を即表示しつつ、並行して
//! `get_live_keymap` を呼ぶ。実機から取れたらライブ表示へ切り替え、
//! 取れない理由(未接続 / ファーム未対応 / ロック中)は UI にそのまま出す。

mod client;
mod framing;
mod labels;
mod proto;

use std::collections::HashMap;

use serde::Serialize;
use tauri::{AppHandle, Manager};

pub use client::StudioError;

/// フロントエンドとの IPC 契約(表示用に変換済みのキーマップ)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveKeymap {
    /// 物理レイアウト名(ZMK の physical layout display-name)
    pub layout_name: String,
    /// キー枠。全レイヤー共通で、単位はキーピッチ(1u = 1.0)
    pub keys: Vec<KeyShape>,
    pub layers: Vec<LiveLayer>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyShape {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// 回転角(度)と回転中心。ZMK は 1/100 度単位で送ってくる
    pub r: f64,
    pub rx: f64,
    pub ry: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveLayer {
    pub id: u32,
    pub name: String,
    /// keys と同じ並び(key position 順)のラベル
    pub bindings: Vec<KeyLabel>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyLabel {
    /// タップ(単押し)の表示
    pub tap: String,
    /// 長押し側の表示(&lt / &mt など)。なければ None
    pub hold: Option<String>,
}

/// `get_live_keymap` の戻り値。status で分岐できるタグ付き表現
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum LiveKeymapResult {
    /// 実機から取得できた
    Ready { keymap: LiveKeymap },
    /// 取得できない(理由は kind)。フロントは画像表示のまま案内を出す
    Unavailable { kind: UnavailableKind, detail: String },
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UnavailableKind {
    /// KobitoKey と BLE 接続できていない
    NotConnected,
    /// ファームが Studio 未対応(CONFIG_ZMK_STUDIO 無効)
    NotSupported,
    /// Studio ロック中(unlock 操作か CONFIG_ZMK_STUDIO_LOCKING=n が必要)
    Locked,
    /// その他(タイムアウト・BLE・プロトコル)
    Error,
}

/// 実機からキーマップを取得して表示用モデルに変換する
#[tauri::command]
pub async fn get_live_keymap(app: AppHandle) -> LiveKeymapResult {
    // BLE 層が未接続なら実機取得は試みない(battery 層の状態が唯一の情報源)
    let connected = app
        .state::<crate::battery::BatteryState>()
        .snapshot()
        .connected;
    if !connected {
        return LiveKeymapResult::Unavailable {
            kind: UnavailableKind::NotConnected,
            detail: "KobitoKey と接続できていません".into(),
        };
    }

    match client::fetch_snapshot().await {
        Ok(snapshot) => match build_live_keymap(snapshot) {
            Ok(keymap) => {
                eprintln!(
                    "[studio] 実機キーマップを取得({} レイヤー、{} キー)",
                    keymap.layers.len(),
                    keymap.keys.len()
                );
                LiveKeymapResult::Ready { keymap }
            }
            Err(detail) => LiveKeymapResult::Unavailable {
                kind: UnavailableKind::Error,
                detail,
            },
        },
        Err(e) => {
            eprintln!("[studio] 実機キーマップを取得できませんでした: {e}");
            LiveKeymapResult::Unavailable {
                kind: match e {
                    StudioError::NotSupported => UnavailableKind::NotSupported,
                    StudioError::Locked => UnavailableKind::Locked,
                    _ => UnavailableKind::Error,
                },
                detail: e.to_string(),
            }
        }
    }
}

/// 実機の生データ(protobuf)を表示用モデルへ変換する
fn build_live_keymap(snapshot: client::StudioSnapshot) -> Result<LiveKeymap, String> {
    let layouts = snapshot.layouts;
    let layout = layouts
        .layouts
        .get(layouts.active_layout_index as usize)
        .or_else(|| layouts.layouts.first())
        .ok_or("ファームが physical layout を返しませんでした")?;

    // ZMK の KeyPhysicalAttrs は 1/100 キーピッチ・1/100 度単位
    let keys: Vec<KeyShape> = layout
        .keys
        .iter()
        .map(|k| KeyShape {
            x: k.x as f64 / 100.0,
            y: k.y as f64 / 100.0,
            width: k.width as f64 / 100.0,
            height: k.height as f64 / 100.0,
            r: k.r as f64 / 100.0,
            rx: k.rx as f64 / 100.0,
            ry: k.ry as f64 / 100.0,
        })
        .collect();

    let layer_names: HashMap<u32, String> = snapshot
        .keymap
        .layers
        .iter()
        .enumerate()
        .map(|(i, l)| (l.id, display_layer_name(l, i)))
        .collect();

    let layers = snapshot
        .keymap
        .layers
        .iter()
        .enumerate()
        .map(|(i, l)| LiveLayer {
            id: l.id,
            name: display_layer_name(l, i),
            bindings: l
                .bindings
                .iter()
                .map(|b| labels::binding_label(b, &snapshot.behaviors, &layer_names))
                .collect(),
        })
        .collect();

    Ok(LiveKeymap {
        layout_name: layout.name.clone(),
        keys,
        layers,
    })
}

/// レイヤー名。ZMK では未設定(空文字)がありうるので index で補う
fn display_layer_name(layer: &proto::keymap::Layer, index: usize) -> String {
    if layer.name.is_empty() {
        format!("Layer {index}")
    } else {
        layer.name.clone()
    }
}
