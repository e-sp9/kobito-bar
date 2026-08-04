//! 電池残量の状態管理。BLE 層(ble.rs)が取得した値をここで一元管理し、
//! フロントエンド(`battery-updated` イベント)とトレイメニューへ配信する。

pub(crate) mod ble;

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

/// フロントエンドとの IPC 契約。`get_battery_status` コマンドの戻り値であり、
/// `battery-updated` イベントのペイロードでもある。
#[derive(Debug, Clone, Default, Serialize)]
pub struct BatteryStatus {
    /// KobitoKey(左手側 = central)と BLE 接続できているか
    pub connected: bool,
    /// 左手側の電池残量(0-100)。未取得なら None
    pub left: Option<u8>,
    /// 右手側の電池残量(0-100)。右手側が左手側と未接続の間は None
    pub right: Option<u8>,
}

impl BatteryStatus {
    fn set_level(&mut self, side: Side, level: Option<u8>) {
        match side {
            Side::Left => self.left = level,
            Side::Right => self.right = level,
        }
    }
}

/// キーボードのどちら半分か
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// managed state。BLE 層が更新し、`get_battery_status` コマンドが読む
pub struct BatteryState(Mutex<BatteryStatus>);

impl BatteryState {
    pub fn snapshot(&self) -> BatteryStatus {
        self.0.lock().unwrap().clone()
    }
}

/// BLE 監視タスクを開始する。アプリと同寿命(停止 API は持たない)
pub fn spawn(app: &AppHandle) {
    app.manage(BatteryState(Mutex::new(BatteryStatus::default())));
    let app = app.clone();
    tauri::async_runtime::spawn(ble::run(app));
}

/// 状態を更新し、フロントエンドとトレイメニューへ配信する
fn publish(app: &AppHandle, mutate: impl FnOnce(&mut BatteryStatus)) {
    let status = {
        let state = app.state::<BatteryState>();
        let mut guard = state.0.lock().unwrap();
        mutate(&mut guard);
        guard.clone()
    };

    let _ = app.emit("battery-updated", &status);

    if let Some(tray) = app.try_state::<crate::tray::TrayHandles<tauri::Wry>>() {
        let _ = tray
            .battery_left
            .set_text(format_level_label("左手", status.left));
        let _ = tray
            .battery_right
            .set_text(format_level_label("右手", status.right));
    }
}

fn format_level_label(side: &str, level: Option<u8>) -> String {
    match level {
        Some(v) => format!("{side}: {v}%"),
        None => format!("{side}: --%"),
    }
}

/// ZMK が報告する生値を表示用に解釈する。右手側(peripheral)が左手側と
/// 未接続の間、ZMK のプロキシは 0 を報告するため、0 は「不明」として扱う
fn interpret_raw_level(raw: Option<u8>) -> Option<u8> {
    match raw {
        None | Some(0) => None,
        Some(v) => Some(v.min(100)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpret_zero_as_unknown() {
        assert_eq!(interpret_raw_level(Some(0)), None);
        assert_eq!(interpret_raw_level(None), None);
    }

    #[test]
    fn interpret_normal_levels_as_is() {
        assert_eq!(interpret_raw_level(Some(1)), Some(1));
        assert_eq!(interpret_raw_level(Some(85)), Some(85));
        assert_eq!(interpret_raw_level(Some(100)), Some(100));
    }

    #[test]
    fn interpret_clamps_over_100() {
        assert_eq!(interpret_raw_level(Some(255)), Some(100));
    }

    #[test]
    fn format_label_with_and_without_level() {
        assert_eq!(format_level_label("左手", Some(85)), "左手: 85%");
        assert_eq!(format_level_label("右手", None), "右手: --%");
    }

    #[test]
    fn set_level_targets_correct_side() {
        let mut status = BatteryStatus::default();
        status.set_level(Side::Left, Some(80));
        status.set_level(Side::Right, Some(60));
        assert_eq!(status.left, Some(80));
        assert_eq!(status.right, Some(60));
        status.set_level(Side::Right, None);
        assert_eq!(status.right, None);
        assert_eq!(status.left, Some(80));
    }
}
