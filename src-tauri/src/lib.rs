pub mod tray;

use serde::Serialize;

/// フロントエンドとの IPC 契約。`get_battery_status` コマンドの戻り値であり、
/// `battery-updated` イベントのペイロードでもある。
#[derive(Debug, Clone, Serialize)]
pub struct BatteryStatus {
    /// KobitoKey(左手側 = central)と BLE 接続できているか
    pub connected: bool,
    /// 左手側の電池残量(0-100)。未取得なら None
    pub left: Option<u8>,
    /// 右手側の電池残量(0-100)。右手側が左手側と未接続の間は None
    pub right: Option<u8>,
}

/// BLE 層(マイルストーン2)が実装されるまではダミー値を返す。
#[tauri::command]
fn get_battery_status() -> BatteryStatus {
    BatteryStatus {
        connected: false,
        left: None,
        right: None,
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![get_battery_status])
        .setup(|app| {
            // Dock(macOS)に出さないトレイ常駐アプリにする
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            tray::setup(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // ウィンドウを閉じてもアプリは終了せず隠すだけ(トレイ常駐を継続)
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            // 全ウィンドウが閉じられても常駐を続ける。
            // トレイメニューの「終了」は app.exit(0) で code が Some になるため通る。
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}
