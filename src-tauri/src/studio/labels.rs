//! BehaviorBinding(数値)→ キーキャップ表示文字列への変換。
//!
//! 解釈は behavior のメタデータ(GetBehaviorDetails)に従う汎用方式:
//! パラメータの型(HidUsage / LayerId / Constant / Range / Nil)を見て
//! ラベル化するため、&kp / &mo / &lt / &mt / &bt などを個別実装せずに済む。
//! ZMK Studio 本家(TS 実装)と同じ考え方。

use std::collections::HashMap;

use super::proto::behaviors::{
    behavior_parameter_value_description::ValueType, BehaviorParameterValueDescription,
    GetBehaviorDetailsResponse,
};
use super::proto::keymap::BehaviorBinding;
use super::KeyLabel;

/// binding 1 個を表示ラベルへ変換する
pub fn binding_label(
    binding: &BehaviorBinding,
    behaviors: &HashMap<i32, GetBehaviorDetailsResponse>,
    layer_names: &HashMap<u32, String>,
) -> KeyLabel {
    let Some(details) = behaviors.get(&binding.behavior_id) else {
        // メタデータ取得に失敗した behavior。素の ID を出すよりは無印にする
        return KeyLabel {
            tap: format!("#{}", binding.behavior_id),
            hold: None,
        };
    };

    // パラメータ型ごとの変換で表現しきれない特別な表示だけ先に処理
    match details.display_name.as_str() {
        "Transparent" => {
            // 下位レイヤーに透過。ZMK 界隈の慣習で ▽
            return KeyLabel {
                tap: "▽".to_string(),
                hold: None,
            };
        }
        "None" => {
            return KeyLabel {
                tap: String::new(),
                hold: None,
            };
        }
        _ => {}
    }

    // metadata は「param1 がこの型のとき param2 はこの型」という組の列。
    // 大半の behavior は 1 組だけ持つ。param1 の値にマッチする組を選ぶ
    let set = details
        .metadata
        .iter()
        .find(|s| find_description(binding.param1, &s.param1).is_some())
        .or_else(|| details.metadata.first());

    let (p1, p2) = match set {
        Some(set) => (
            param_label(binding.param1, &set.param1, layer_names),
            param_label(binding.param2, &set.param2, layer_names),
        ),
        None => (None, None),
    };

    match (p1, p2) {
        // 2 パラメータ behavior(&lt, &mt などの hold-tap 系)は
        // ZMK の慣習で param1 = hold、param2 = tap
        (Some(hold), Some(tap)) => KeyLabel {
            tap,
            hold: Some(hold),
        },
        (Some(tap), None) => KeyLabel { tap, hold: None },
        // パラメータなし(&bootloader 等)は behavior 名を短縮表示
        _ => KeyLabel {
            tap: short_behavior_name(&details.display_name),
            hold: None,
        },
    }
}

/// param 値を、その位置のパラメータ型記述に従ってラベル化する。
/// 記述が空(パラメータを取らない位置)なら None
fn param_label(
    value: u32,
    descriptions: &[BehaviorParameterValueDescription],
    layer_names: &HashMap<u32, String>,
) -> Option<String> {
    if descriptions.is_empty() {
        return None;
    }
    match find_description(value, descriptions).and_then(|d| d.value_type.as_ref()) {
        Some(ValueType::Nil(_)) | None => None,
        Some(ValueType::HidUsage(_)) => Some(hid_usage_label(value)),
        Some(ValueType::LayerId(_)) => Some(
            layer_names
                .get(&value)
                .cloned()
                .unwrap_or_else(|| format!("L{value}")),
        ),
        // 定数はメタデータ側の名前が表示名(例: &bt の "Clear All" 等)
        Some(ValueType::Constant(_)) => find_description(value, descriptions)
            .map(|d| d.name.clone())
            .filter(|n| !n.is_empty())
            .or_else(|| Some(value.to_string())),
        Some(ValueType::Range(_)) => Some(value.to_string()),
    }
}

/// value にマッチするパラメータ記述を探す(Constant は値一致、Range は範囲内、
/// HidUsage / LayerId / Nil は型として常にマッチ)
fn find_description<'a>(
    value: u32,
    descriptions: &'a [BehaviorParameterValueDescription],
) -> Option<&'a BehaviorParameterValueDescription> {
    descriptions.iter().find(|d| match d.value_type.as_ref() {
        Some(ValueType::Constant(c)) => *c == value,
        Some(ValueType::Range(r)) => (r.min..=r.max).contains(&(value as i32)),
        Some(ValueType::HidUsage(_) | ValueType::LayerId(_) | ValueType::Nil(_)) => true,
        None => false,
    })
}

fn short_behavior_name(display_name: &str) -> String {
    match display_name {
        "Bootloader" => "BOOT".to_string(),
        "Reset" => "RST".to_string(),
        "Studio Unlock" => "🔓".to_string(),
        other => other.to_string(),
    }
}

// --- HID usage のラベル化 -------------------------------------------------
//
// binding param のエンコード(zmk の dt-bindings/zmk/hid_usage_pages.h と
// modifiers.h で確認済み):
//   bits 24-31: implicit modifiers / bits 16-23: usage page / bits 0-15: usage id

const PAGE_KEYBOARD: u32 = 0x07;
const PAGE_CONSUMER: u32 = 0x0C;

const MOD_LCTL: u32 = 0x01;
const MOD_LSFT: u32 = 0x02;
const MOD_LALT: u32 = 0x04;
const MOD_LGUI: u32 = 0x08;
const MOD_RCTL: u32 = 0x10;
const MOD_RSFT: u32 = 0x20;
const MOD_RALT: u32 = 0x40;
const MOD_RGUI: u32 = 0x80;

fn hid_usage_label(value: u32) -> String {
    let mods = (value >> 24) & 0xFF;
    let page = (value >> 16) & 0xFF;
    let id = value & 0xFFFF;

    let base = match page {
        PAGE_KEYBOARD => keyboard_usage_name(id),
        PAGE_CONSUMER => consumer_usage_name(id),
        _ => None,
    }
    .unwrap_or_else(|| format!("0x{page:02X}:{id:02X}"));

    if mods == 0 {
        return base;
    }

    // Shift 単独修飾は US 配列の shifted 記号に変換(記号レイヤーの見やすさ優先)
    if mods == MOD_LSFT || mods == MOD_RSFT {
        if let Some(shifted) = shifted_symbol(&base) {
            return shifted.to_string();
        }
    }

    let mut prefix = String::new();
    if mods & (MOD_LCTL | MOD_RCTL) != 0 {
        prefix.push('⌃');
    }
    if mods & (MOD_LSFT | MOD_RSFT) != 0 {
        prefix.push('⇧');
    }
    if mods & (MOD_LALT | MOD_RALT) != 0 {
        prefix.push('⌥');
    }
    if mods & (MOD_LGUI | MOD_RGUI) != 0 {
        prefix.push('⌘');
    }
    format!("{prefix}{base}")
}

/// HID Keyboard/Keypad page(0x07)の主要 usage
fn keyboard_usage_name(id: u32) -> Option<String> {
    let name = match id {
        // A-Z(0x04-0x1D)
        0x04..=0x1D => return Some(char::from(b'A' + (id - 0x04) as u8).to_string()),
        // 1-9, 0(0x1E-0x27)
        0x1E..=0x26 => return Some(char::from(b'1' + (id - 0x1E) as u8).to_string()),
        0x27 => "0",
        0x28 => "Enter",
        0x29 => "Esc",
        0x2A => "BS",
        0x2B => "Tab",
        0x2C => "Space",
        0x2D => "-",
        0x2E => "=",
        0x2F => "[",
        0x30 => "]",
        0x31 => "\\",
        0x32 => "#",
        0x33 => ";",
        0x34 => "'",
        0x35 => "`",
        0x36 => ",",
        0x37 => ".",
        0x38 => "/",
        0x39 => "Caps",
        // F1-F12(0x3A-0x45)
        0x3A..=0x45 => return Some(format!("F{}", id - 0x39)),
        0x46 => "PrtSc",
        0x47 => "ScrLk",
        0x48 => "Pause",
        0x49 => "Ins",
        0x4A => "Home",
        0x4B => "PgUp",
        0x4C => "Del",
        0x4D => "End",
        0x4E => "PgDn",
        0x4F => "→",
        0x50 => "←",
        0x51 => "↓",
        0x52 => "↑",
        0x53 => "NumLk",
        0x54 => "KP/",
        0x55 => "KP*",
        0x56 => "KP-",
        0x57 => "KP+",
        0x58 => "KP⏎",
        // KP1-KP9(0x59-0x61)
        0x59..=0x61 => return Some(format!("KP{}", id - 0x58)),
        0x62 => "KP0",
        0x63 => "KP.",
        0x64 => "\\",
        0x65 => "Menu",
        0x66 => "Pwr",
        0x67 => "KP=",
        // F13-F24(0x68-0x73)
        0x68..=0x73 => return Some(format!("F{}", id - 0x68 + 13)),
        0x7F => "Mute",
        0x80 => "Vol+",
        0x81 => "Vol-",
        // 国際キー・言語キー(日本語配列 / macOS 日本語入力)
        0x87 => "ろ",
        0x88 => "かな",
        0x89 => "¥",
        0x8A => "変換",
        0x8B => "無変換",
        0x90 => "かな",  // LANG1(macOS: かな)
        0x91 => "英数",  // LANG2(macOS: 英数)
        0xE0 => "Ctrl",
        0xE1 => "Shift",
        0xE2 => "Alt",
        0xE3 => "Cmd",
        0xE4 => "Ctrl",
        0xE5 => "Shift",
        0xE6 => "Alt",
        0xE7 => "Cmd",
        _ => return None,
    };
    Some(name.to_string())
}

/// HID Consumer page(0x0C)の主要 usage
fn consumer_usage_name(id: u32) -> Option<String> {
    let name = match id {
        0x6F => "Bri+",
        0x70 => "Bri-",
        0xB5 => "⏭",
        0xB6 => "⏮",
        0xB7 => "⏹",
        0xCD => "⏯",
        0xE2 => "Mute",
        0xE9 => "Vol+",
        0xEA => "Vol-",
        _ => return None,
    };
    Some(name.to_string())
}

/// US 配列で Shift を押したときの記号(Shift 単独修飾の表示用)
fn shifted_symbol(base: &str) -> Option<&'static str> {
    Some(match base {
        "1" => "!",
        "2" => "@",
        "3" => "#",
        "4" => "$",
        "5" => "%",
        "6" => "^",
        "7" => "&",
        "8" => "*",
        "9" => "(",
        "0" => ")",
        "-" => "_",
        "=" => "+",
        "[" => "{",
        "]" => "}",
        "\\" => "|",
        ";" => ":",
        "'" => "\"",
        "`" => "~",
        "," => "<",
        "." => ">",
        "/" => "?",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ZMK_HID_USAGE(page, id) 相当
    fn usage(page: u32, id: u32) -> u32 {
        (page << 16) | id
    }

    #[test]
    fn plain_letters_and_digits() {
        assert_eq!(hid_usage_label(usage(0x07, 0x04)), "A");
        assert_eq!(hid_usage_label(usage(0x07, 0x1D)), "Z");
        assert_eq!(hid_usage_label(usage(0x07, 0x1E)), "1");
        assert_eq!(hid_usage_label(usage(0x07, 0x27)), "0");
    }

    #[test]
    fn shifted_single_modifier_becomes_symbol() {
        // LS(N2) = @(us 配列)
        let ls_2 = (MOD_LSFT << 24) | usage(0x07, 0x1F);
        assert_eq!(hid_usage_label(ls_2), "@");
        // RS でも同様
        let rs_semi = (MOD_RSFT << 24) | usage(0x07, 0x33);
        assert_eq!(hid_usage_label(rs_semi), ":");
    }

    #[test]
    fn other_modifiers_render_as_prefix() {
        let lg_c = (MOD_LGUI << 24) | usage(0x07, 0x06);
        assert_eq!(hid_usage_label(lg_c), "⌘C");
        // Shift でも shifted 記号がない文字はプレフィックス表示
        let ls_a = (MOD_LSFT << 24) | usage(0x07, 0x04);
        assert_eq!(hid_usage_label(ls_a), "⇧A");
    }

    #[test]
    fn consumer_and_lang_keys() {
        assert_eq!(hid_usage_label(usage(0x0C, 0xE9)), "Vol+");
        assert_eq!(hid_usage_label(usage(0x07, 0x90)), "かな");
        assert_eq!(hid_usage_label(usage(0x07, 0x91)), "英数");
    }

    #[test]
    fn unknown_usage_falls_back_to_hex() {
        assert_eq!(hid_usage_label(usage(0x07, 0x9E)), "0x07:9E");
    }
}
