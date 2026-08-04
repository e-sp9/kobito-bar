pub mod battery;
pub mod keymap;
pub mod studio;
pub mod tray;

#[tauri::command]
fn get_battery_status(state: tauri::State<'_, battery::BatteryState>) -> battery::BatteryStatus {
    state.snapshot()
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            get_battery_status,
            keymap::get_keymap_images,
            keymap::show_keymap_window,
            studio::get_live_keymap
        ])
        .setup(|app| {
            // Dock(macOS)に出さないトレイ常駐アプリにする
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            {
                use tauri::Manager;
                app.manage(keymap::KeymapState::default());
            }
            tray::setup(app.handle())?;
            // トレイメニュー項目(TrayHandles)を manage した後に起動する
            battery::spawn(app.handle());

            // 開発ビルドでは起動時からポップアップを表示する。トレイが表示されない
            // 環境(WSLg にはトレイ表示ホストがない)でも動作確認できるように
            #[cfg(debug_assertions)]
            {
                use tauri::Manager;
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            match event {
                // ウィンドウを閉じてもアプリは終了せず隠すだけ(トレイ常駐を継続)
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    let _ = window.hide();
                    api.prevent_close();
                }
                // ポップアップはフォーカスが外れたら隠す(トレイポップアップの流儀)。
                // debug ビルドでは無効: WSLg にはトレイがなく、一度隠れると
                // 再表示する手段がないため(実機確認は release の dev-windows.sh で)
                #[cfg(not(debug_assertions))]
                tauri::WindowEvent::Focused(false) if window.label() == "main" => {
                    use tauri::Manager;
                    if window.is_visible().unwrap_or(false) {
                        let _ = window.hide();
                        // 直後に届くトレイクリックで再表示しないよう記録する
                        window
                            .app_handle()
                            .state::<tray::PopupState>()
                            .mark_hidden_by_focus_out();
                    }
                }
                _ => {}
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
