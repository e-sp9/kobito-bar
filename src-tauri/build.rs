fn main() {
    // ZMK Studio RPC の proto(proto/README.md 参照)を Rust 型にコンパイルする。
    // protoc はシステムに要求せず vendored バイナリを使う(WSL/CI どちらでも動く)
    std::env::set_var(
        "PROTOC",
        protoc_bin_vendored::protoc_bin_path().expect("vendored protoc がありません"),
    );
    prost_build::Config::new()
        .compile_protos(&["proto/zmk/studio.proto"], &["proto/zmk"])
        .expect("ZMK Studio proto のコンパイルに失敗");

    tauri_build::build()
}
