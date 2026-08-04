//! ZMK Studio RPC の BLE クライアント。
//!
//! 1 回の取得セッション(デバイス探索 → RPC characteristic 購読 → 一連の
//! リクエスト)を `fetch_snapshot()` が担う。常時接続は持たない —
//! キーマップはウィンドウを開いたときに読めれば十分で、GATT 接続自体は
//! OS が HID/battery 用に維持しているものへ相乗りする。
//!
//! プロトコル(zmk の app/src/studio/ で確認済み):
//! - characteristic は write + indicate(bluest の notify() は properties を
//!   見て indicate を自動選択する)
//! - リクエスト/応答とも protobuf を SOF/ESC/EOF でフレーミング
//! - 応答の request_id で相関を取る。Notification は非同期に混ざる

use std::collections::{BTreeSet, HashMap};

use bluest::{Adapter, Characteristic, Device};
use futures_util::{Stream, StreamExt};
use prost::Message;
use tokio::time::{timeout, Duration};
use uuid::Uuid;

use super::framing::{encode_frame, FrameDecoder};
use super::proto;
use crate::battery::ble::KEYBOARD_NAME;

/// ZMK Studio の GATT UUID(zmk: app/src/studio/uuid.h)
const STUDIO_SERVICE_UUID: Uuid = Uuid::from_u128(0x00000000_0196_6107_c967_c5cfb1c2482a);
const STUDIO_RPC_CHRC_UUID: Uuid = Uuid::from_u128(0x00000001_0196_6107_c967_c5cfb1c2482a);

/// 1 リクエストの応答待ちタイムアウト
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);
/// max_write_len が取れないときの保守的なチャンクサイズ(最小 ATT MTU 23 - 3)
const WRITE_CHUNK_FALLBACK: usize = 20;

#[derive(Debug)]
pub enum StudioError {
    /// KobitoKey は接続済みだが Studio service を公開していない
    /// (ファームの CONFIG_ZMK_STUDIO が無効)
    NotSupported,
    /// ファームが locked 状態(CONFIG_ZMK_STUDIO_LOCKING が有効で未 unlock)
    Locked,
    Timeout,
    Ble(String),
    Protocol(String),
}

impl std::fmt::Display for StudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotSupported => write!(f, "ファームウェアが ZMK Studio に対応していません"),
            Self::Locked => write!(f, "キーボードが Studio ロック中です"),
            Self::Timeout => write!(f, "キーボードからの応答がありません"),
            Self::Ble(e) => write!(f, "BLE エラー: {e}"),
            Self::Protocol(e) => write!(f, "プロトコルエラー: {e}"),
        }
    }
}

/// 実機から取得した生データ一式
pub struct StudioSnapshot {
    pub keymap: proto::keymap::Keymap,
    pub layouts: proto::keymap::PhysicalLayouts,
    pub behaviors: HashMap<i32, proto::behaviors::GetBehaviorDetailsResponse>,
}

pub async fn fetch_snapshot() -> Result<StudioSnapshot, StudioError> {
    let adapter = Adapter::default()
        .await
        .ok_or_else(|| StudioError::Ble("Bluetooth アダプタが見つかりません".into()))?;
    adapter
        .wait_available()
        .await
        .map_err(|e| StudioError::Ble(e.to_string()))?;

    let device = find_studio_device(&adapter).await?;
    adapter
        .connect_device(&device)
        .await
        .map_err(|e| StudioError::Ble(e.to_string()))?;

    let chrc = find_rpc_characteristic(&device).await?;
    let notifications = chrc
        .notify()
        .await
        .map_err(|e| StudioError::Ble(format!("indicate の購読に失敗: {e}")))?;
    let mut session = Session {
        chrc: &chrc,
        rx: notifications,
        decoder: FrameDecoder::new(),
        next_id: 1,
    };

    // SECURED な get_keymap を呼ぶ前にロック状態を確認して分かりやすく報告する
    // (CONFIG_ZMK_STUDIO_LOCKING=n のファームは常に UNLOCKED を返す)
    if session.get_lock_state().await? == proto::core::LockState::ZmkStudioCoreLockStateLocked {
        return Err(StudioError::Locked);
    }

    let keymap = session.get_keymap().await?;
    let layouts = session.get_physical_layouts().await?;

    // 表示に必要な behavior のメタデータだけ取得する(重複を除いて数個〜十数個)
    let ids: BTreeSet<i32> = keymap
        .layers
        .iter()
        .flat_map(|l| l.bindings.iter().map(|b| b.behavior_id))
        .collect();
    let mut behaviors = HashMap::new();
    for id in ids {
        match session.get_behavior_details(id).await {
            Ok(details) => {
                behaviors.insert(id, details);
            }
            // 1 個の失敗で全体を諦めない(そのキーだけ簡易表示になる)
            Err(e) => eprintln!("[studio] behavior {id} の詳細取得に失敗: {e}"),
        }
    }

    Ok(StudioSnapshot {
        keymap,
        layouts,
        behaviors,
    })
}

/// Studio service を公開している KobitoKey を探す。
/// Battery service では見えている(= 接続はある)前提で呼ばれるため、
/// ここで見つからなければ「ファームが Studio 未対応」と解釈する
async fn find_studio_device(adapter: &Adapter) -> Result<Device, StudioError> {
    let devices = adapter
        .connected_devices_with_services(&[STUDIO_SERVICE_UUID])
        .await
        .map_err(|e| StudioError::Ble(e.to_string()))?;
    devices
        .into_iter()
        .find(|d| d.name().map(|n| n == KEYBOARD_NAME).unwrap_or(false))
        .ok_or(StudioError::NotSupported)
}

async fn find_rpc_characteristic(device: &Device) -> Result<Characteristic, StudioError> {
    for service in device
        .services()
        .await
        .map_err(|e| StudioError::Ble(e.to_string()))?
    {
        if service.uuid() != STUDIO_SERVICE_UUID {
            continue;
        }
        for chrc in service
            .characteristics()
            .await
            .map_err(|e| StudioError::Ble(e.to_string()))?
        {
            if chrc.uuid() == STUDIO_RPC_CHRC_UUID {
                return Ok(chrc);
            }
        }
    }
    Err(StudioError::NotSupported)
}

struct Session<'a, S> {
    chrc: &'a Characteristic,
    rx: S,
    decoder: FrameDecoder,
    next_id: u32,
}

impl<S> Session<'_, S>
where
    S: Stream<Item = Result<Vec<u8>, bluest::Error>> + Unpin,
{
    /// リクエストを送り、request_id が一致する応答を待つ。
    /// 途中に混ざる Notification は読み飛ばす
    async fn request(
        &mut self,
        subsystem: proto::studio::request::Subsystem,
    ) -> Result<proto::studio::request_response::Subsystem, StudioError> {
        let id = self.next_id;
        self.next_id += 1;
        let req = proto::studio::Request {
            request_id: id,
            subsystem: Some(subsystem),
        };
        let frame = encode_frame(&req.encode_to_vec());
        let chunk_size = self
            .chrc
            .max_write_len()
            .ok()
            .filter(|n| *n > 0)
            .unwrap_or(WRITE_CHUNK_FALLBACK);
        for chunk in frame.chunks(chunk_size) {
            self.chrc
                .write(chunk)
                .await
                .map_err(|e| StudioError::Ble(format!("write に失敗: {e}")))?;
        }

        loop {
            let item = timeout(RESPONSE_TIMEOUT, self.rx.next())
                .await
                .map_err(|_| StudioError::Timeout)?
                .ok_or_else(|| StudioError::Ble("indicate ストリームが終了しました".into()))?
                .map_err(|e| StudioError::Ble(e.to_string()))?;

            for payload in self.decoder.push(&item) {
                let resp = proto::studio::Response::decode(payload.as_slice())
                    .map_err(|e| StudioError::Protocol(format!("応答の decode に失敗: {e}")))?;
                match resp.r#type {
                    Some(proto::studio::response::Type::RequestResponse(rr)) => {
                        if rr.request_id != id {
                            continue;
                        }
                        return match rr.subsystem {
                            // meta はエラー通知(UNLOCK_REQUIRED 等)
                            Some(proto::studio::request_response::Subsystem::Meta(m)) => {
                                Err(meta_error(m))
                            }
                            Some(subsystem) => Ok(subsystem),
                            None => {
                                Err(StudioError::Protocol("応答に subsystem がありません".into()))
                            }
                        };
                    }
                    // 取得セッション中の通知(ロック状態変化など)は無視する
                    Some(proto::studio::response::Type::Notification(_)) | None => continue,
                }
            }
        }
    }

    async fn get_lock_state(&mut self) -> Result<proto::core::LockState, StudioError> {
        let resp = self
            .request(proto::studio::request::Subsystem::Core(
                proto::core::Request {
                    request_type: Some(proto::core::request::RequestType::GetLockState(true)),
                },
            ))
            .await?;
        match resp {
            proto::studio::request_response::Subsystem::Core(proto::core::Response {
                response_type:
                    Some(proto::core::response::ResponseType::GetLockState(state)),
            }) => proto::core::LockState::try_from(state)
                .map_err(|_| StudioError::Protocol(format!("不明な LockState: {state}"))),
            other => Err(unexpected("get_lock_state", &other)),
        }
    }

    async fn get_keymap(&mut self) -> Result<proto::keymap::Keymap, StudioError> {
        let resp = self
            .request(proto::studio::request::Subsystem::Keymap(
                proto::keymap::Request {
                    request_type: Some(proto::keymap::request::RequestType::GetKeymap(true)),
                },
            ))
            .await?;
        match resp {
            proto::studio::request_response::Subsystem::Keymap(proto::keymap::Response {
                response_type: Some(proto::keymap::response::ResponseType::GetKeymap(keymap)),
            }) => Ok(keymap),
            other => Err(unexpected("get_keymap", &other)),
        }
    }

    async fn get_physical_layouts(
        &mut self,
    ) -> Result<proto::keymap::PhysicalLayouts, StudioError> {
        let resp = self
            .request(proto::studio::request::Subsystem::Keymap(
                proto::keymap::Request {
                    request_type: Some(proto::keymap::request::RequestType::GetPhysicalLayouts(
                        true,
                    )),
                },
            ))
            .await?;
        match resp {
            proto::studio::request_response::Subsystem::Keymap(proto::keymap::Response {
                response_type:
                    Some(proto::keymap::response::ResponseType::GetPhysicalLayouts(layouts)),
            }) => Ok(layouts),
            other => Err(unexpected("get_physical_layouts", &other)),
        }
    }

    async fn get_behavior_details(
        &mut self,
        behavior_id: i32,
    ) -> Result<proto::behaviors::GetBehaviorDetailsResponse, StudioError> {
        let resp = self
            .request(proto::studio::request::Subsystem::Behaviors(
                proto::behaviors::Request {
                    request_type: Some(
                        proto::behaviors::request::RequestType::GetBehaviorDetails(
                            proto::behaviors::GetBehaviorDetailsRequest {
                                behavior_id: behavior_id as u32,
                            },
                        ),
                    ),
                },
            ))
            .await?;
        match resp {
            proto::studio::request_response::Subsystem::Behaviors(
                proto::behaviors::Response {
                    response_type:
                        Some(proto::behaviors::response::ResponseType::GetBehaviorDetails(d)),
                },
            ) => Ok(d),
            other => Err(unexpected("get_behavior_details", &other)),
        }
    }
}

fn meta_error(m: proto::meta::Response) -> StudioError {
    match m.response_type {
        Some(proto::meta::response::ResponseType::SimpleError(code)) => {
            match proto::meta::ErrorConditions::try_from(code) {
                Ok(proto::meta::ErrorConditions::UnlockRequired) => StudioError::Locked,
                Ok(cond) => StudioError::Protocol(format!("ファームがエラーを返しました: {cond:?}")),
                Err(_) => StudioError::Protocol(format!("不明なエラーコード: {code}")),
            }
        }
        _ => StudioError::Protocol("ファームが空の meta 応答を返しました".into()),
    }
}

fn unexpected(what: &str, got: &proto::studio::request_response::Subsystem) -> StudioError {
    let kind = match got {
        proto::studio::request_response::Subsystem::Meta(_) => "meta",
        proto::studio::request_response::Subsystem::Core(_) => "core",
        proto::studio::request_response::Subsystem::Behaviors(_) => "behaviors",
        proto::studio::request_response::Subsystem::Keymap(_) => "keymap",
    };
    StudioError::Protocol(format!("{what} への応答が想定外でした({kind})"))
}
