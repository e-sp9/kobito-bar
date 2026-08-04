//! prost が build.rs で生成した ZMK Studio RPC の型。
//! proto 定義は `src-tauri/proto/zmk/`(vendored。proto/README.md 参照)。
//!
//! 生成コードはパッケージ間参照を `super::core::…` の形で行うため、
//! ここで zmk.* パッケージを兄弟モジュールとして include する。

pub mod meta {
    include!(concat!(env!("OUT_DIR"), "/zmk.meta.rs"));
}

pub mod core {
    include!(concat!(env!("OUT_DIR"), "/zmk.core.rs"));
}

pub mod behaviors {
    include!(concat!(env!("OUT_DIR"), "/zmk.behaviors.rs"));
}

pub mod keymap {
    include!(concat!(env!("OUT_DIR"), "/zmk.keymap.rs"));
}

pub mod studio {
    include!(concat!(env!("OUT_DIR"), "/zmk.studio.rs"));
}
