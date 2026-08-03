use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Runtime,
};
use tauri_plugin_autostart::ManagerExt;

/// 電池残量表示メニュー項目のハンドル。BLE 層(マイルストーン2)が
/// managed state からこれを取り出してトレイメニューの表示を更新する。
pub struct TrayHandles<R: Runtime> {
    pub battery_left: MenuItem<R>,
    pub battery_right: MenuItem<R>,
}

pub fn setup<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let battery_left = MenuItem::with_id(app, "battery-left", "左手: --%", false, None::<&str>)?;
    let battery_right = MenuItem::with_id(app, "battery-right", "右手: --%", false, None::<&str>)?;
    let keymap = MenuItem::with_id(app, "keymap", "キーマップを表示", true, None::<&str>)?;
    let autostart_enabled = app.autolaunch().is_enabled().unwrap_or(false);
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "ログイン時に起動",
        true,
        autostart_enabled,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "KobitoBar を終了", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &battery_left,
            &battery_right,
            &PredefinedMenuItem::separator(app)?,
            &keymap,
            &autostart,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let autostart_item = autostart.clone();
    let mut builder = TrayIconBuilder::with_id("kobito-tray")
        .tooltip("KobitoBar")
        .menu(&menu)
        // 左クリックはポップアップのトグルに使う(メニューは右クリック)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "keymap" => {
                // キーマップ画面(マイルストーン4)まではポップアップを表示しておく
                show_popup(app);
            }
            "autostart" => {
                let autolaunch = app.autolaunch();
                let result = if autolaunch.is_enabled().unwrap_or(false) {
                    autolaunch.disable()
                } else {
                    autolaunch.enable()
                };
                if let Err(e) = result {
                    eprintln!("自動起動の切り替えに失敗: {e}");
                }
                // 失敗時もチェック状態を実際の設定と一致させる
                let _ = autostart_item.set_checked(autolaunch.is_enabled().unwrap_or(false));
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_popup(tray.app_handle());
            }
        });

    // TODO(マイルストーン5): 小人のドット絵フレームに差し替え、
    // macOS では icon_as_template(true) のモノクロアイコンにする
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;

    app.manage(TrayHandles {
        battery_left,
        battery_right,
    });
    Ok(())
}

fn toggle_popup<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            // TODO(マイルストーン3): トレイアイコン脇への位置合わせ
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

fn show_popup<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}
