//! bluest による KobitoKey の BLE 監視。
//!
//! - PC と BLE 接続するのは左手側(central)のみ。1 本の GATT 接続で
//!   Battery Service(0x180F)の Battery Level characteristic(0x2A19)が
//!   2 つ見える: CUD descriptor(0x2901)が付いている方が右手側のプロキシ
//!   (ZMK は "Peripheral 0" という CUD を付け、central 自身には付けない)
//! - キーボードは OS が HID としてボンディング・接続済みで advertise して
//!   いないため、BLE スキャンは行わず接続済みデバイス一覧のポーリングで
//!   検出する(zmk-battery-center 方式)
//! - KobitoKey は無操作 30 分でディープスリープする。切断を検出したら
//!   接続済み一覧に再登場するのを待って自動で再購読する

use bluest::btuuid::descriptors::CHARACTERISTIC_USER_DESCRIPTION;
use bluest::{Adapter, Characteristic, ConnectionEvent, Device};
use futures_util::stream::{SelectAll, StreamExt};
use tauri::AppHandle;
use tokio::time::{sleep, Duration};
use uuid::Uuid;

use super::{interpret_raw_level, publish, BatteryStatus, Side};

/// CONFIG_ZMK_KEYBOARD_NAME の値
const KEYBOARD_NAME: &str = "KobitoKey";
const BATTERY_SERVICE_UUID: Uuid = Uuid::from_u128(0x0000180F_0000_1000_8000_00805F9B34FB);
const BATTERY_LEVEL_UUID: Uuid = Uuid::from_u128(0x00002A19_0000_1000_8000_00805F9B34FB);

/// 接続済みデバイス一覧に KobitoKey が現れるのを待つ間隔
const DEVICE_POLL_INTERVAL: Duration = Duration::from_secs(5);
/// 切断・エラー後に監視をやり直すまでの間隔
const RETRY_INTERVAL: Duration = Duration::from_secs(2);
/// Bluetooth アダプタが使えないときの再確認間隔
const ADAPTER_RETRY_INTERVAL: Duration = Duration::from_secs(30);

pub async fn run(app: AppHandle) {
    loop {
        let Some(adapter) = Adapter::default().await else {
            // WSL2 など Bluetooth スタックがない環境ではここに留まり続ける
            eprintln!("[ble] Bluetooth アダプタが見つかりません");
            sleep(ADAPTER_RETRY_INTERVAL).await;
            continue;
        };
        if let Err(e) = adapter.wait_available().await {
            eprintln!("[ble] Bluetooth アダプタが利用できません: {e}");
            sleep(ADAPTER_RETRY_INTERVAL).await;
            continue;
        }
        eprintln!("[ble] Bluetooth アダプタ OK、KobitoKey を探します");

        monitor_once(&app, &adapter).await;

        // 切断またはエラーで戻ってきた。状態を落として再接続を待つ
        publish(&app, |s| *s = BatteryStatus::default());
        sleep(RETRY_INTERVAL).await;
    }
}

/// KobitoKey を見つけて接続し、左右の残量を購読する。切断・エラーで戻る
async fn monitor_once(app: &AppHandle, adapter: &Adapter) {
    let device = loop {
        match find_kobitokey(adapter).await {
            Some(d) => break d,
            None => sleep(DEVICE_POLL_INTERVAL).await,
        }
    };

    eprintln!("[ble] KobitoKey を検出、接続します");
    if let Err(e) = adapter.connect_device(&device).await {
        eprintln!("[ble] KobitoKey への接続に失敗: {e}");
        return;
    }

    // 切断検出用。macOS では接続確立後に購読する必要がある
    let mut conn_events = match adapter.device_connection_events(&device).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("接続イベントの購読に失敗: {e}");
            return;
        }
    };

    let sides = match discover_battery_characteristics(&device).await {
        Ok(s) if !s.is_empty() => s,
        Ok(_) => {
            eprintln!("[ble] Battery Level characteristic が見つかりません");
            return;
        }
        Err(e) => {
            eprintln!("[ble] Battery Service の探索に失敗: {e}");
            return;
        }
    };
    eprintln!(
        "[ble] Battery Level characteristic {} 個: {:?}",
        sides.len(),
        sides.iter().map(|(side, _)| *side).collect::<Vec<_>>()
    );

    // ZMK の notify はデフォルト 60 秒間隔のため、初回値は read で取る
    let mut initial = BatteryStatus {
        connected: true,
        left: None,
        right: None,
    };
    for (side, ch) in &sides {
        match ch.read().await {
            Ok(value) => {
                eprintln!("[ble] 初回読み取り {side:?}: 生値 {:?}", value.first());
                initial.set_level(*side, interpret_raw_level(value.first().copied()));
            }
            Err(e) => eprintln!("[ble] 初回読み取りに失敗 ({side:?}): {e}"),
        }
    }
    publish(app, move |s| *s = initial);

    // 左右の notify を 1 本のストリームに合流して購読する
    let mut notifications = SelectAll::new();
    for (side, ch) in &sides {
        match ch.notify().await {
            Ok(stream) => {
                let side = *side;
                notifications.push(stream.map(move |item| (side, item)).boxed());
            }
            Err(e) => eprintln!("[ble] notify の購読に失敗 ({side:?}): {e}"),
        }
    }
    if notifications.is_empty() {
        return;
    }
    eprintln!("[ble] notify 購読開始({} 本)", notifications.len());

    loop {
        tokio::select! {
            item = notifications.next() => match item {
                Some((side, Ok(data))) => {
                    eprintln!("[ble] notify {side:?}: 生値 {:?}", data.first());
                    let level = interpret_raw_level(data.first().copied());
                    publish(app, move |s| s.set_level(side, level));
                }
                Some((side, Err(e))) => {
                    eprintln!("[ble] notify ストリームのエラー ({side:?}): {e}");
                    return;
                }
                // 全ストリーム終了 = 切断
                None => {
                    eprintln!("[ble] notify ストリームが終了(切断)");
                    return;
                }
            },
            event = conn_events.next() => {
                if !matches!(event, Some(ConnectionEvent::Connected)) {
                    eprintln!("[ble] 切断イベントを検出");
                    return;
                }
            }
        }
    }
}

/// 接続済みデバイス一覧から KobitoKey(左手側)を探す
async fn find_kobitokey(adapter: &Adapter) -> Option<Device> {
    let devices = match adapter
        .connected_devices_with_services(&[BATTERY_SERVICE_UUID, BATTERY_LEVEL_UUID])
        .await
    {
        Ok(devices) => devices,
        Err(e) => {
            eprintln!("[ble] 接続済みデバイスの列挙に失敗: {e}");
            return None;
        }
    };
    let names: Vec<String> = devices
        .iter()
        .map(|d| d.name().unwrap_or_else(|_| "(名前不明)".to_string()))
        .collect();
    eprintln!(
        "[ble] BatteryService を持つ接続済みデバイス {} 台: {names:?}",
        devices.len()
    );
    devices
        .into_iter()
        .find(|d| d.name().map(|n| n == KEYBOARD_NAME).unwrap_or(false))
}

/// Battery Level characteristic を列挙し、CUD descriptor の有無で左右を割り当てる
async fn discover_battery_characteristics(
    device: &Device,
) -> Result<Vec<(Side, Characteristic)>, bluest::Error> {
    let mut found = Vec::new();
    for service in device.services().await? {
        if service.uuid() != BATTERY_SERVICE_UUID {
            continue;
        }
        for ch in service.characteristics().await? {
            if ch.uuid() != BATTERY_LEVEL_UUID {
                continue;
            }
            let has_cud = ch
                .descriptors()
                .await?
                .iter()
                .any(|d| d.uuid() == CHARACTERISTIC_USER_DESCRIPTION);
            found.push((side_from_cud_presence(has_cud), ch));
        }
    }
    Ok(found)
}

/// CUD descriptor が付いているのは右手側(peripheral)のプロキシだけ。
/// 内容("Peripheral 0")ではなく有無で判定する — 将来 ZMK 側で文言が
/// 変わっても壊れず、read 失敗で左右を取り違えることもない
fn side_from_cud_presence(has_cud: bool) -> Side {
    if has_cud {
        Side::Right
    } else {
        Side::Left
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cud_presence_maps_to_right() {
        assert_eq!(side_from_cud_presence(true), Side::Right);
        assert_eq!(side_from_cud_presence(false), Side::Left);
    }
}
