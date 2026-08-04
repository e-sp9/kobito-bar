//! ZMK Studio RPC のメッセージフレーミング。
//! zmk の app/src/studio/msg_framing.c と対称の実装(SOF/ESC/EOF)。

pub const SOF: u8 = 0xAB;
pub const ESC: u8 = 0xAC;
pub const EOF: u8 = 0xAD;

/// ペイロードを 1 フレームにエンコードする。
/// データ中の SOF/ESC/EOF には ESC を前置する(ZMK 側 rpc.c と同じ規則)
pub fn encode_frame(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 2);
    out.push(SOF);
    for &b in payload {
        if b == SOF || b == ESC || b == EOF {
            out.push(ESC);
        }
        out.push(b);
    }
    out.push(EOF);
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    AwaitingData,
    Escaped,
    /// エスケープなしの SOF が来た等の異常。EOF か SOF で回復する
    /// (zmk の FRAMING_STATE_ERR と同じ挙動)
    Err,
}

/// BLE indicate は MTU 単位の細切れで届くため、チャンクをまたいで
/// フレームを組み立てるストリーミングデコーダ
pub struct FrameDecoder {
    state: State,
    buf: Vec<u8>,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self {
            state: State::Idle,
            buf: Vec::new(),
        }
    }

    /// 受信チャンクを流し込み、完成したフレーム(ペイロード)を返す
    pub fn push(&mut self, chunk: &[u8]) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();
        for &b in chunk {
            match self.state {
                State::Idle => {
                    if b == SOF {
                        self.state = State::AwaitingData;
                        self.buf.clear();
                    }
                    // SOF 以外はフレーム外のゴミとして無視
                }
                State::AwaitingData => match b {
                    SOF => {
                        // エスケープなしの SOF がデータ中に現れるのは異常
                        self.state = State::Err;
                        self.buf.clear();
                    }
                    ESC => self.state = State::Escaped,
                    EOF => {
                        self.state = State::Idle;
                        frames.push(std::mem::take(&mut self.buf));
                    }
                    _ => self.buf.push(b),
                },
                State::Escaped => {
                    self.buf.push(b);
                    self.state = State::AwaitingData;
                }
                State::Err => match b {
                    EOF => self.state = State::Idle,
                    SOF => {
                        self.state = State::AwaitingData;
                        self.buf.clear();
                    }
                    _ => {}
                },
            }
        }
        frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_simple() {
        let payload = b"\x08\x01\x2a\x02\x08\x01";
        let mut dec = FrameDecoder::new();
        let frames = dec.push(&encode_frame(payload));
        assert_eq!(frames, vec![payload.to_vec()]);
    }

    #[test]
    fn roundtrip_with_framing_bytes_in_payload() {
        // ペイロードに SOF/ESC/EOF そのものが含まれるケース
        let payload = [0xAB, 0xAC, 0xAD, 0x01, 0xAB];
        let encoded = encode_frame(&payload);
        // 3 + エスケープ 4 個 + データ 5 = 12 バイト
        assert_eq!(encoded.len(), 2 + payload.len() + 4);
        let mut dec = FrameDecoder::new();
        assert_eq!(dec.push(&encoded), vec![payload.to_vec()]);
    }

    #[test]
    fn decodes_across_chunks() {
        // BLE の MTU 分割を想定して 1 バイトずつ流す
        let payload = [0x12, 0xAD, 0x34];
        let encoded = encode_frame(&payload);
        let mut dec = FrameDecoder::new();
        let mut frames = Vec::new();
        for b in encoded {
            frames.extend(dec.push(&[b]));
        }
        assert_eq!(frames, vec![payload.to_vec()]);
    }

    #[test]
    fn decodes_multiple_frames_in_one_chunk() {
        let mut data = encode_frame(&[1, 2]);
        data.extend(encode_frame(&[3]));
        let mut dec = FrameDecoder::new();
        assert_eq!(dec.push(&data), vec![vec![1, 2], vec![3]]);
    }

    #[test]
    fn ignores_noise_outside_frames() {
        let mut data = vec![0x00, 0xFF];
        data.extend(encode_frame(&[9]));
        let mut dec = FrameDecoder::new();
        assert_eq!(dec.push(&data), vec![vec![9]]);
    }

    #[test]
    fn recovers_from_unescaped_sof_mid_data() {
        // データ途中の生 SOF は ERR 状態になり、進行中のフレームは破棄。
        // ERR 中のデータは捨てられ、EOF で Idle に回復する(zmk と同じ)
        let mut dec = FrameDecoder::new();
        assert!(dec.push(&[SOF, 0x01, SOF]).is_empty());
        assert!(dec.push(&[0x02, EOF]).is_empty());
        // 回復後は通常どおり受信できる
        assert_eq!(dec.push(&encode_frame(&[0x03])), vec![vec![0x03]]);
    }

    #[test]
    fn err_state_restarts_on_sof() {
        // ERR 中に SOF が来た場合は即座に新フレームを開始する(zmk と同じ)
        let mut dec = FrameDecoder::new();
        assert!(dec.push(&[SOF, 0x01, SOF]).is_empty());
        assert_eq!(dec.push(&[SOF, 0x05, EOF]), vec![vec![0x05]]);
    }
}
