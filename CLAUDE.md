# CLAUDE.md

このリポジトリは **KobitoBar** — 自作分割キーボード「KobitoKey」のコンパニオン常駐アプリ。
ユーザーとのやり取りは日本語で行うこと。

## プロダクト概要

RunCat のように macOS のメニューバー / Windows のシステムトレイに常駐し、以下を提供する:

1. **左右それぞれの電池残量表示**(コア機能)
2. **キーマップ一覧表示**(レイヤーごとのキー配置図)
3. 低残量時の通知
4. RunCat 風の世界観: トレイアイコンは小人(kobito)のドット絵で、電池残量に応じて状態が変わる(元気に働く → 座って休む → 寝る)。モチーフはグリム童話「靴屋の小人」(夜中にこっそり働いてくれる小人)

- リポジトリ名: `kobito-bar` / アプリ表示名: **KobitoBar**
- KobitoKey ユーザーコミュニティへの配布を見据える(GitHub Releases)

## 技術スタック(決定済み)

| 項目 | 選定 |
|---|---|
| フレームワーク | Tauri v2 + Rust(1 コードベースで macOS / Windows 両対応) |
| フロントエンド | React + TypeScript + Vite |
| パッケージマネージャ | pnpm |
| BLE | bluest(当初計画の btleplug から変更 — 接続済みデバイス列挙・CUD descriptor 読み取り・切断イベント検出を標準サポートし OS API 直叩きが不要。参考実装 zmk-battery-center と同じ) |
| 自動起動 | tauri-plugin-autostart |
| バンドル ID | `com.esp9.kobitobar` |

**参考実装: [kot149/zmk-battery-center](https://github.com/kot149/zmk-battery-center)** — Tauri v2 + BLE で ZMK キーボードの左右電池残量を表示する同種アプリ。BLE 層の書き方、トレイ常駐、再接続処理で行き詰まったらこのソースを参照すること。KobitoBar の差別化点はキーマップ表示と小人の世界観。

## KobitoKey の技術仕様(ファームウェア検証済み)

ZMK ファームウェアの左右分割キーボード(4 行 × 10 列、PMW3610 トラックボール搭載、Seeed XIAO BLE / nRF52840)。

- 設定リポジトリ: `e-sp9/KobitoKey_QWERTY`(このマシンのローカル: `/home/ryota/ghq/github.com/e-sp9/KobitoKey_QWERTY`)
- **アプリに必要なファーム設定は検証済みで、ファーム側の変更は一切不要**(v1 の範囲では)

検証済みの設定値(`config/boards/shields/KobitoKey/KobitoKey_left.conf` ほか):

- BLE デバイス名: `CONFIG_ZMK_KEYBOARD_NAME="KobitoKey"`
- 左手側が central(`CONFIG_ZMK_SPLIT_ROLE_CENTRAL=y`)。**PC と BLE 接続するのは左手側のみ**。右手側は左手側とだけ通信する
- `CONFIG_ZMK_BATTERY_REPORTING=y`(左右とも)
- `CONFIG_ZMK_SPLIT_BLE_CENTRAL_BATTERY_LEVEL_FETCHING=y` + `CONFIG_ZMK_SPLIT_BLE_CENTRAL_BATTERY_LEVEL_PROXY=y`(左)
- 無操作 30 分でディープスリープ(`CONFIG_ZMK_SLEEP=y`, `CONFIG_ZMK_IDLE_SLEEP_TIMEOUT=1800000`)。復帰はキー押下のみ(トラックボールでは復帰しない)

> **注意(引き継ぎメモからの訂正)**: ZMK Studio(`CONFIG_ZMK_STUDIO`)は現在のファームでは**無効**。KobitoKey_QWERTY の commit `22ec120` "Optimize firmware: ... drop Studio" で削除済み。v2 のキーマップ動的取得をやる際はファーム側での再有効化が必要。

### 電池残量の読み方(BLE)

- PC ↔ 左手側の 1 本の BLE 接続だけで左右両方の残量が取れる
- 標準 GATT **Battery Service(UUID `0x180F`)の Battery Level characteristic(UUID `0x2A19`)が 2 つ**存在する
  - 1 つは左手側(central)自身の残量
  - もう 1 つは右手側(peripheral)のプロキシ
- 区別は各 characteristic の **Characteristic User Description descriptor(UUID `0x2901`)** を読む
- 値は 0–100 の uint8。read と notify(subscribe)の両方に対応。ZMK の報告間隔はデフォルト 60 秒
- 右手側が左手側と未接続の間は、プロキシ値が 0 や古い値になりうる点を UI で考慮する(「不明」表示など)

### キーマップ(表示機能用)

4 レイヤー構成。KobitoKey_QWERTY リポジトリの `images/layer0.png`〜`layer3.png` に各レイヤーの配置図がある。`scripts/generate_keymap_images.py`(Pillow 製)が `config/KobitoKey.keymap` から自動生成しているため、実キーマップとの同期が保証されている。

- Layer 0 — Default(QWERTY)
- Layer 1 — NUMBER(Space 長押し。数字・Bluetooth プロファイル切替)
- Layer 2 — SYMBOL(Enter 長押し。記号・矢印)
- Layer 3 — MOUSE(トラックボール用。マウス操作)

**v1 は静的表示**: 上記画像を利用する。取得方式は GitHub raw から取得+ローカルキャッシュを推奨(キーマップ更新に追従できる)。オフライン時はキャッシュ/同梱画像にフォールバック。

**v2(将来)**: ZMK Studio RPC プロトコル(protobuf over USB/BLE。zmkfirmware/zmk-studio-messages で定義が公開されている)で実キーマップを動的取得。※上記の通りファーム側の Studio 再有効化が前提。

## 実装上の注意点(調査済み)

1. **接続済みデバイスの検出が肝**: キーボードは OS が既に HID として接続しており advertise していないため、BLE スキャンには出てこない。bluest の `connected_devices_with_services()` がこれを解決する(macOS は CoreBluetooth の接続済みペリフェラル取得、Windows はペアリング済みデバイス列挙)。このため btleplug ではなく bluest を採用した。スキャンは一切行わず、接続済み一覧を 5 秒間隔でポーリングして検出する(スキャンは Windows でシステム全体を重くする — zmk-battery-center の知見)。
2. **再接続ハンドリング**: KobitoKey は無操作 30 分でディープスリープする。切断 → 復帰時に自動で再接続し、GATT notify を再購読するループを最初から設計に入れる。
3. **macOS 権限・配布**: `NSBluetoothAlwaysUsageDescription` が必要(`src-tauri/Info.plist` に記載済み)。未署名アプリは Gatekeeper に弾かれるため、配布時は Developer ID 署名+公証、または README に回避手順(quarantine 解除)を記載する。
4. **Windows**: ペアリング済みデバイスへのアクセスに特別な権限は不要。未署名だと SmartScreen 警告が出る程度。
5. **トレイアイコンの動的描画**: フレーム画像を複数用意し `set_icon` で差し替える方式(RunCat と同じ)。macOS ではテンプレートアイコン(モノクロ)推奨。

## マイルストーン

1. ✅ Tauri v2 雛形(トレイ常駐、自動起動設定)
2. ✅ BLE 層: KobitoKey 検出 → Battery Service の 2 characteristic 読み取り+notify 購読 → CUD で左右識別 → 切断・再接続処理(**実機未検証** — WSL2 では型チェックのみ。macOS / Windows での動作確認が必要)
3. ⬜ トレイポップアップ UI(左右残量表示。トレイ横への位置合わせ、フォーカスアウトで閉じる等の作り込みはここで)
4. ⬜ キーマップ表示画面(4 レイヤーの画像表示)
5. ⬜ 小人ドット絵アニメーション(残量連動。macOS はテンプレートアイコン化)
6. ⬜ GitHub Actions で macOS / Windows ビルド → GitHub Releases 配布

## リポジトリ構成と開発コマンド

```
kobito-bar/
├── src/                  # フロントエンド(React + TS)。トレイポップアップ UI
├── src-tauri/
│   ├── src/lib.rs        # アプリ本体(Builder、ウィンドウ挙動、IPC コマンド)
│   ├── src/tray.rs       # トレイアイコン・メニュー
│   ├── src/battery/      # BLE 層。mod.rs = 状態管理・配信、ble.rs = bluest 監視ループ
│   ├── Info.plist        # macOS 用 Bluetooth 権限記述(ビルド時にマージされる)
│   ├── capabilities/     # Tauri v2 ACL
│   └── icons/            # `pnpm tauri icon` で app-icon.png から生成
├── scripts/
│   ├── generate-icon.mjs     # プレースホルダの小人ドット絵を生成(依存なし Node)
│   └── dev-windows.sh        # Windows 用クロスビルド+ Windows 側で起動(BLE/トレイ実機確認)
└── app-icon.png          # アイコンのソース画像
```

- `pnpm install` — 依存インストール
- `pnpm tauri dev` — 開発起動(GUI 環境が必要)
- `pnpm tauri build` — 配布ビルド
- `pnpm build` — フロントエンドのみの型チェック+ビルド
- `cargo check`(`src-tauri/` 内)— Rust の型チェック
- アイコン再生成: `node scripts/generate-icon.mjs && pnpm tauri icon`

### 実装メモ(雛形時点の設計)

- トレイ左クリック = ポップアップ(`main` ウィンドウ)のトグル、右クリック = メニュー
- メニューの「左手/右手: --%」項目は `tray::TrayHandles`(managed state)経由で BLE 層が更新する
- **BLE 層(マイルストーン 2 実装済み)**: `battery::spawn()` が `BatteryState` を manage して監視タスクを起動(アプリと同寿命、停止 API なし)。`battery::publish()` が state 更新 + `battery-updated` emit + トレイメニュー `set_text` を一元化
  - 左右識別は CUD descriptor(0x2901)の**有無**で行う(あり = 右手プロキシ、なし = 左手自身)。ZMK はプロキシにだけ CUD `"Peripheral 0"` を付ける(central_bas_proxy.c で確認済み)。中身は見ない — read 失敗による左右取り違えを防ぐため
  - 生値 0 は「不明」(None)扱い(右手が左手と未接続の間、ZMK プロキシは 0 を報告する)
  - 接続直後に read で初期値取得(ZMK の notify はデフォルト 60 秒間隔のため)、以後は notify
  - 切断検出は `device_connection_events` + notify ストリーム終了の両方。検出後は状態を落とし 2 秒後に接続済み一覧のポーリング(5 秒間隔)へ戻る = ディープスリープ復帰も自動で拾う
- ウィンドウを閉じても hide のみで常駐継続。終了はトレイメニュー「KobitoBar を終了」→ `app.exit(0)`。`RunEvent::ExitRequested` は `code.is_none()` のときだけ `prevent_exit()`
- macOS では `ActivationPolicy::Accessory` で Dock 非表示
- フロント ⇔ Rust の IPC 契約: `get_battery_status` コマンド+ `battery-updated` イベント(現状はダミー実装)
- 現在のアイコンは `scripts/generate-icon.mjs` によるプレースホルダの小人ドット絵。マイルストーン 5 で本番アートに差し替える

## 開発環境の注意(このマシン = WSL2)

- WSL2 には Bluetooth スタックがないため、WSL 内での BLE 実機確認は不可
- webkit2gtk-4.1 / GTK3 はインストール済みで、**このマシンで `pnpm build` と `cargo check` が通ることは確認済み**(2026-08-04)
- **UI 開発は WSLg で可能**: `pnpm tauri dev` で Windows デスクトップ上にウィンドウが表示される(確認済み 2026-08-04)。ただし WSLg にはトレイ表示ホストがなく**トレイアイコンは出ない**。このため debug ビルドは起動時にポップアップを自動表示する(`lib.rs` の `#[cfg(debug_assertions)]`)。libEGL / MESA / Gtk-CRITICAL 警告は WSLg のソフトウェアレンダリング関連で無害
- **BLE・トレイ込みの実機確認は `scripts/dev-windows.sh`**(確認済み 2026-08-04): cargo-xwin で Windows 用 .exe をクロスビルド → Windows 側 `%TEMP%\KobitoBar` にコピー → WSL interop で起動。Windows の実 Bluetooth スタック・実トレイで動く。ホットリロードはなし。cargo-xwin と x86_64-pc-windows-msvc target はインストール済み
- bluest は Linux backend(BlueZ / bluer)を持つため**無条件依存**にしてある。WSL2 でも BLE コード込みで `cargo check`・`cargo test` が通る(実行時は `Adapter::default()` が None になり 30 秒間隔で待機し続けるだけで壊れない)。`cargo check` に bluer v0.16 の future-incompat 警告が出るが依存側の問題で実害なし
