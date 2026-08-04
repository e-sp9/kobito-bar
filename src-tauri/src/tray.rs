use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Monitor, PhysicalPosition, Position, Rect, Runtime, Size, WebviewWindow,
};
use tauri_plugin_autostart::ManagerExt;

/// フォーカスアウトによる hide 直後、この時間内のトレイクリックは
/// 「閉じる操作」とみなして再表示しない(Windows ではポップアップ表示中に
/// トレイをクリックすると、Click イベントより先にフォーカスアウトが届く)
const FOCUS_OUT_TOGGLE_GRACE: Duration = Duration::from_millis(400);

/// トレイアイコンとポップアップの間隔(論理 px)
const POPUP_GAP: f64 = 8.0;

/// トレイ関連のハンドル。BLE 層(マイルストーン2)が managed state から
/// これを取り出してメニューの残量表示とホバー時ツールチップを更新する。
pub struct TrayHandles<R: Runtime> {
    pub tray: TrayIcon<R>,
    pub battery_left: MenuItem<R>,
    pub battery_right: MenuItem<R>,
}

/// トレイポップアップの位置合わせとトグル制御の状態
#[derive(Default)]
pub struct PopupState {
    /// 最後に観測したトレイアイコンの矩形(物理 px)。表示位置の計算に使う。
    /// トレイイベントが来ない環境(WSLg)では None のままで位置合わせをスキップ
    tray_rect: Mutex<Option<Rect>>,
    /// フォーカスアウトで hide された時刻
    hidden_at: Mutex<Option<Instant>>,
}

impl PopupState {
    fn record_tray_rect(&self, rect: Rect) {
        *self.tray_rect.lock().unwrap() = Some(rect);
    }

    fn tray_rect(&self) -> Option<Rect> {
        *self.tray_rect.lock().unwrap()
    }

    /// フォーカスアウトで hide したことを記録する(lib.rs のウィンドウイベントから)
    pub fn mark_hidden_by_focus_out(&self) {
        *self.hidden_at.lock().unwrap() = Some(Instant::now());
    }

    fn recently_hidden_by_focus_out(&self) -> bool {
        self.hidden_at
            .lock()
            .unwrap()
            .is_some_and(|t| t.elapsed() < FOCUS_OUT_TOGGLE_GRACE)
    }
}

pub fn setup<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    // トレイイベントより先に用意しておく(イベントハンドラが state() で取り出す)
    app.manage(PopupState::default());

    let battery_left = MenuItem::with_id(app, "battery-left", "左手: --%", false, None::<&str>)?;
    let battery_right = MenuItem::with_id(app, "battery-right", "右手: --%", false, None::<&str>)?;
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
            &autostart,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let autostart_item = autostart.clone();
    let mut builder = TrayIconBuilder::with_id("kobito-tray")
        // BLE 層が接続後に「kobitobar — L 82% / R 76%」形式へ更新する
        .tooltip("kobitobar — scanning")
        .menu(&menu)
        // 左クリックはポップアップのトグルに使う(メニューは右クリック)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
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
            let app = tray.app_handle();
            // どのイベントからでもトレイアイコンの位置を記録しておく。
            // メニュー経由の表示(右クリック→キーマップ)でも位置合わせできるように
            let rect = match &event {
                TrayIconEvent::Click { rect, .. }
                | TrayIconEvent::DoubleClick { rect, .. }
                | TrayIconEvent::Enter { rect, .. }
                | TrayIconEvent::Move { rect, .. }
                | TrayIconEvent::Leave { rect, .. } => Some(*rect),
                _ => None,
            };
            if let (Some(rect), Some(state)) = (rect, app.try_state::<PopupState>()) {
                state.record_tray_rect(rect);
            }

            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_popup(app);
            }
        });

    // TODO(マイルストーン5): 小人のドット絵フレームに差し替え、
    // macOS では icon_as_template(true) のモノクロアイコンにする
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    let tray = builder.build(app)?;

    app.manage(TrayHandles {
        tray,
        battery_left,
        battery_right,
    });
    Ok(())
}

fn toggle_popup<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
    } else {
        // フォーカスアウトの hide 直後に届いたクリックは「閉じる操作」の後半。
        // ここで再表示するとポップアップを閉じられなくなる
        if app.state::<PopupState>().recently_hidden_by_focus_out() {
            return;
        }
        show_popup(app);
    }
}

fn show_popup<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if let Some(rect) = app.state::<PopupState>().tray_rect() {
        position_near_tray(&window, &rect);
    }
    let _ = window.show();
    let _ = window.set_focus();
}

/// ポップアップをトレイアイコンの脇へ移動する。位置計算に失敗しても
/// 表示自体は行う(呼び出し元が show する)ため、失敗は黙って無視する
fn position_near_tray<R: Runtime>(window: &WebviewWindow<R>, tray_rect: &Rect) {
    // tray-icon は全 OS で物理 px・上原点に変換済みの rect を届ける
    let (Position::Physical(tray_pos), Size::Physical(tray_size)) =
        (tray_rect.position, tray_rect.size)
    else {
        return;
    };
    let tray = (
        tray_pos.x as f64,
        tray_pos.y as f64,
        tray_size.width as f64,
        tray_size.height as f64,
    );

    let Some(monitor) = monitor_containing(window, (tray.0 + tray.2 / 2.0, tray.1 + tray.3 / 2.0))
    else {
        return;
    };
    let work_area = monitor.work_area();
    let work = (
        work_area.position.x as f64,
        work_area.position.y as f64,
        work_area.size.width as f64,
        work_area.size.height as f64,
    );

    // ウィンドウの物理サイズは現在いるモニタの scale 基準なので、
    // 論理サイズに戻してから表示先モニタの scale で換算し直す
    let Ok(win_size) = window.outer_size() else {
        return;
    };
    let win_logical = win_size.to_logical::<f64>(window.scale_factor().unwrap_or(1.0));
    let scale = monitor.scale_factor();
    let win = (win_logical.width * scale, win_logical.height * scale);

    let (x, y) = popup_position(tray, work, win, POPUP_GAP * scale);
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

/// 指定した点(物理 px)を含むモニタを探す。どのモニタにも含まれなければ
/// プライマリモニタへフォールバックする。
/// tauri の monitor_from_point は macOS だと論理座標を要求し OS 差があるため、
/// 物理座標で統一されている position/size を使って自前で判定する
fn monitor_containing<R: Runtime>(window: &WebviewWindow<R>, point: (f64, f64)) -> Option<Monitor> {
    let (x, y) = point;
    window
        .available_monitors()
        .ok()?
        .into_iter()
        .find(|m| {
            let pos = m.position();
            let size = m.size();
            x >= pos.x as f64
                && x < pos.x as f64 + size.width as f64
                && y >= pos.y as f64
                && y < pos.y as f64 + size.height as f64
        })
        .or_else(|| window.primary_monitor().ok().flatten())
}

/// ポップアップの左上座標(物理 px)を計算する。
/// - x: トレイアイコン中央にウィンドウ中央を合わせ、作業領域内にクランプ
/// - y: トレイが作業領域の中心より上(macOS のメニューバー)ならアイコンの下、
///   中心より下(Windows のタスクバー)ならアイコンの上に出す
///
/// 引数の rect はいずれも (x, y, 幅, 高さ)、win は (幅, 高さ)。すべて物理 px
fn popup_position(
    tray: (f64, f64, f64, f64),
    work: (f64, f64, f64, f64),
    win: (f64, f64),
    gap: f64,
) -> (i32, i32) {
    let (tray_x, tray_y, tray_w, tray_h) = tray;
    let (work_x, work_y, work_w, work_h) = work;
    let (win_w, win_h) = win;

    // clamp() は min > max で panic するため min/max の連結で書く
    // (ウィンドウが作業領域より大きい異常系では左/上端に寄せる)
    let x = (tray_x + tray_w / 2.0 - win_w / 2.0)
        .min(work_x + work_w - win_w)
        .max(work_x);

    let tray_above_center = tray_y + tray_h / 2.0 < work_y + work_h / 2.0;
    let y = if tray_above_center {
        tray_y + tray_h + gap
    } else {
        tray_y - win_h - gap
    };
    let y = y.min(work_y + work_h - win_h).max(work_y);

    (x.round() as i32, y.round() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    // macOS 風: メニューバー(作業領域の上)のトレイ → アイコンの下に出る
    #[test]
    fn opens_below_menu_bar_tray() {
        let (x, y) = popup_position(
            (1000.0, 0.0, 60.0, 48.0),    // トレイ: 画面上端
            (0.0, 50.0, 2880.0, 1750.0),  // 作業領域: メニューバー除く
            (648.0, 768.0),               // ウィンドウ 324x384 @2x
            16.0,
        );
        assert_eq!(x, 1000 + 30 - 648 / 2); // トレイ中央に合わせる
        assert_eq!(y, 48 + 16); // アイコン下端 + gap
    }

    // Windows 風: タスクバー(作業領域の下)のトレイ → アイコンの上に出る
    #[test]
    fn opens_above_taskbar_tray() {
        let (x, y) = popup_position(
            (1000.0, 1044.0, 24.0, 24.0), // トレイ: 下端タスクバー内
            (0.0, 0.0, 1920.0, 1040.0),   // 作業領域: タスクバー除く
            (324.0, 384.0),
            8.0,
        );
        assert_eq!(x, 1000 + 12 - 324 / 2);
        assert_eq!(y, 1044 - 384 - 8); // アイコン上端 - ウィンドウ高 - gap
    }

    // 画面右端のトレイでもウィンドウが作業領域からはみ出さない
    #[test]
    fn clamps_to_right_edge() {
        let (x, _) = popup_position(
            (2850.0, 0.0, 30.0, 48.0),
            (0.0, 50.0, 2880.0, 1750.0),
            (648.0, 768.0),
            16.0,
        );
        assert_eq!(x, 2880 - 648); // 右端に張り付く
    }

    // 左端でも同様(マルチモニタで作業領域が負座標の場合も含む)
    #[test]
    fn clamps_to_left_edge_with_negative_origin() {
        let (x, _) = popup_position(
            (-1910.0, 1044.0, 24.0, 24.0),
            (-1920.0, 0.0, 1920.0, 1040.0),
            (324.0, 384.0),
            8.0,
        );
        assert_eq!(x, -1920); // 作業領域の左端でクランプ
    }
}
