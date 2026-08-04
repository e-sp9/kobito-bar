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
- 無操作 1 時間でディープスリープ(`CONFIG_ZMK_SLEEP=y`, `CONFIG_ZMK_IDLE_SLEEP_TIMEOUT=3600000`。以前は 30 分だった)。復帰はキー押下のみ(トラックボールでは復帰しない)

> **注意**: ZMK Studio(`CONFIG_ZMK_STUDIO`)は現在のファームでは**無効**(KobitoKey_QWERTY の commit `22ec120` "Optimize firmware: ... drop Studio" で削除済み)。v2 のライブキーマップを動かすには **KobitoKey_left.conf(左手側のみ)に 2 行追記**してファームを書き込む:
> `CONFIG_ZMK_STUDIO=y` と `CONFIG_ZMK_STUDIO_LOCKING=n`。
> BLE transport(`ZMK_STUDIO_TRANSPORT_BLE`)は default y なので指定不要。Studio の GATT characteristic は READ/WRITE_ENCRYPT でボンド済みデバイスしかアクセスできないため、LOCKING=n でもペアリング済み PC 以外からは読めない。LOCKING を有効のまま残す場合はキーマップに `&studio_unlock` キーが必要で、KobitoBar は locked を検出すると案内を出して画像表示にフォールバックする。

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

**v2(アプリ側実装済み 2026-08-04)**: ZMK Studio RPC(protobuf over BLE GATT)で実機からキーマップ・物理レイアウト・behavior メタデータを動的取得し、SVG でキーマップを描画する。取得できないとき(未接続 / ファーム未対応 / ロック中)は理由を表示して v1 の画像へ自動フォールバック。※実機 E2E は上記のファーム側 Studio 有効化が前提。

## 実装上の注意点(調査済み)

1. **接続済みデバイスの検出が肝**: キーボードは OS が既に HID として接続しており advertise していないため、BLE スキャンには出てこない。bluest の `connected_devices_with_services()` がこれを解決する(macOS は CoreBluetooth の接続済みペリフェラル取得、Windows はペアリング済みデバイス列挙)。このため btleplug ではなく bluest を採用した。スキャンは一切行わず、接続済み一覧を 5 秒間隔でポーリングして検出する(スキャンは Windows でシステム全体を重くする — zmk-battery-center の知見)。
2. **再接続ハンドリング**: KobitoKey は無操作 1 時間でディープスリープする。切断 → 復帰時に自動で再接続し、GATT notify を再購読するループを最初から設計に入れる。
3. **macOS 権限・配布**: `NSBluetoothAlwaysUsageDescription` が必要(`src-tauri/Info.plist` に記載済み)。未署名アプリは Gatekeeper に弾かれるため、配布時は Developer ID 署名+公証、または README に回避手順(quarantine 解除)を記載する。
4. **Windows**: ペアリング済みデバイスへのアクセスに特別な権限は不要。未署名だと SmartScreen 警告が出る程度。
5. **トレイアイコンの動的描画**: フレーム画像を複数用意し `set_icon` で差し替える方式(RunCat と同じ)。macOS ではテンプレートアイコン(モノクロ)推奨。

## マイルストーン

1. ✅ Tauri v2 雛形(トレイ常駐、自動起動設定)
2. ✅ BLE 層: KobitoKey 検出 → Battery Service の 2 characteristic 読み取り+notify 購読 → CUD で左右識別 → 切断・再接続処理(**Windows 実機で検証済み 2026-08-04**: 検出・接続・CUD 左右識別・read 初期値(左 90%/右 91%)・notify 受信まで確認。notify は ZMK 仕様で残量が変化したときのみ届く。**未検証**: 切断→再接続(ディープスリープ復帰)、macOS)
3. ✅ トレイポップアップ UI(トレイ横への位置合わせ、フォーカスアウトで閉じる、透明ウィンドウ+角丸カード化。**Windows 実機で目視確認済み 2026-08-04**: 位置合わせ・フォーカスアウト・トグル・透明表示 OK。**未検証**: macOS)
4. ✅ キーマップ表示画面(4 レイヤーの画像表示。**Windows 実機で GitHub raw 取得+キャッシュ保存まで確認済み 2026-08-04**。**未検証**: オフラインフォールバック、macOS)
   - **v2 ライブ取得(ZMK Studio RPC)もアプリ側実装済み**(2026-08-04)。実機 E2E は**ファームの Studio 有効化待ち**(上記「注意」の 2 行を KobitoKey_left.conf に追記 → ビルド → 左手側に書き込み)。ファーム未対応時の画像フォールバックは Windows 実機で検証済み
   - **2026-08-05 ポップアップへ統合**: Claude Design のモック(`KobitoBar Windows.dc.html`)を実装し、キーマップ表示を専用ウィンドウからトレイポップアップ内へ移動(keymap ウィンドウ・`show_keymap_window`・トレイメニュー「キーマップを表示」は撤去)。全状態(scan / live / fallback+low)は WSLg で目視確認済み。**未検証**: Windows / macOS 実機
5. ⬜ 小人ドット絵アニメーション(残量連動。macOS はテンプレートアイコン化)
6. ⬜ GitHub Actions で macOS / Windows ビルド → GitHub Releases 配布

## リポジトリ構成と開発コマンド

```
kobito-bar/
├── src/                  # フロントエンド(React + TS)
│   ├── App.tsx           # 単一ビュー(PopupView を表示するだけ)
│   ├── PopupView.tsx     # トレイポップアップ(電池残量+キーマップ統合、ターミナル風)
│   └── styles.css        # デザイントークン(KobitoBar Windows.dc.html 由来)
├── src-tauri/
│   ├── src/lib.rs        # アプリ本体(Builder、ウィンドウ挙動、IPC コマンド)
│   ├── src/tray.rs       # トレイアイコン・メニュー・ポップアップ位置合わせ
│   ├── src/battery/      # BLE 層。mod.rs = 状態管理・配信、ble.rs = bluest 監視ループ
│   ├── src/keymap.rs     # v1: キーマップ画像の取得・キャッシュ・配信
│   ├── src/studio/       # v2: ZMK Studio RPC(BLE)による実機キーマップ取得
│   ├── proto/            # zmk-studio-messages の proto 定義(vendored)
│   ├── resources/keymaps/ # 同梱キーマップ画像(オフライン初回のフォールバック)
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

### 実装メモ

- トレイ左クリック = ポップアップ(`main` ウィンドウ)のトグル、右クリック = メニュー。ウィンドウは main の 1 枚だけ(キーマップもポップアップ内)
- メニューの「左手/右手: --%」項目とホバー時ツールチップ(`kobitobar — L 82% / R 76%`)は `tray::TrayHandles`(managed state)経由で BLE 層が更新する
- **BLE 層(マイルストーン 2 実装済み)**: `battery::spawn()` が `BatteryState` を manage して監視タスクを起動(アプリと同寿命、停止 API なし)。`battery::publish()` が state 更新 + `battery-updated` emit + トレイメニュー `set_text` を一元化
  - 左右識別は CUD descriptor(0x2901)の**有無**で行う(あり = 右手プロキシ、なし = 左手自身)。ZMK はプロキシにだけ CUD `"Peripheral 0"` を付ける(central_bas_proxy.c で確認済み)。中身は見ない — read 失敗による左右取り違えを防ぐため
  - 生値 0 は「不明」(None)扱い(右手が左手と未接続の間、ZMK プロキシは 0 を報告する)
  - 接続直後に read で初期値取得(ZMK の notify はデフォルト 60 秒間隔のため)、以後は notify
  - 切断検出は `device_connection_events` + notify ストリーム終了の両方。検出後は状態を落とし 2 秒後に接続済み一覧のポーリング(5 秒間隔)へ戻る = ディープスリープ復帰も自動で拾う
- **ポップアップ UI(マイルストーン 3 実装済み、2026-08-05 に Claude Design のターミナル風デザインへ全面刷新)**:
  - デザインは claude.ai/design プロジェクトの `KobitoBar Windows.dc.html`(モノスペース・ダーク #16181D・hud green #5BE18B・四隅ブラケット)。トークンは `src/styles.css` の `--kb-*`。フォントは Cascadia Mono → JetBrains Mono → ui-monospace のフォールバック(Web フォントは読み込まない)
  - カードは 406px 幅固定・内容は上から keymap → battery → ステータスフッター(上向きに開くため % がトレイ側=下に来る)。ウィンドウは 454×448 で、カードは Windows では下端・macOS では上端に寄せる(`navigator.userAgent` の Macintosh 判定で `.anchor-top`)。状態によりカード高さが変わる(scan < live < fallback)ぶんは透明領域
  - 状態は 4 種: scan(未接続: 探索中 placeholder のみ)/ live(実機 SVG ミニキーマップ+レイヤータブ)/ loading(タブ+静的画像、理由行なし)/ fallback(理由 1 行+静的画像)。低残量 ≤20% は amber+「! low」チップ、右手未リンクは「--」+「○ not linked」チップ(色以外の手掛かりを併用)
  - 位置合わせ: トレイイベント(Click/Enter/Move/Leave)の rect を `tray::PopupState`(managed state)に記録し、表示時に `popup_position()`(純関数・テスト付き)で計算。トレイアイコン中央にウィンドウ中央を合わせてモニタ作業領域内にクランプ、トレイが作業領域中心より上なら下に(macOS メニューバー)、下なら上に(Windows タスクバー)出す
  - 座標系: トレイ rect は全 OS で物理 px・上原点(tray-icon が backend で変換済み)。モニタ特定は position/size による自前の内包判定 — tauri の `monitor_from_point` は macOS が論理座標・Windows が物理座標で OS 差があるため使わない
  - フォーカスアウトで hide は **release ビルドのみ**(`#[cfg(not(debug_assertions))]`)— WSLg にはトレイがなく一度隠れると再表示できないため。実機確認は release でビルドする `scripts/dev-windows.sh` で可能
  - hide 直後 400ms 以内のトレイクリックは「閉じる操作」として無視(`PopupState::hidden_at`)。Windows ではポップアップ表示中のトレイクリックが「フォーカスアウト → Click イベント」の順で届き、対策なしだとトグルで閉じられなくなる
  - ウィンドウは `transparent: true` + `shadow: false` + CSS 角丸カード(影も CSS)。macOS の webview 透明化には `macOSPrivateApi: true` と tauri の `macos-private-api` feature が必要 — App Store 配布は不可になるが配布は GitHub Releases なので問題ない
  - `visibleOnAllWorkspaces: true` で macOS の別 Space にいても現在の Space に表示される
- **キーマップ表示(マイルストーン 4 実装済み、2026-08-05 にポップアップ内へ統合)**: ポップアップの keymap セクションに 4 レイヤーをタブ表示(専用ウィンドウは撤去)。表示エリアは 142px 高で、live = SVG ミニキーマップ / fallback = 静的画像
  - 配信は stale-while-revalidate: `get_keymap_images` コマンドがキャッシュ(app_data_dir/keymaps)→ 同梱(Resource/keymaps)の順で即返し、裏で GitHub raw(`KobitoKey_QWERTY/main/images`)から取得 → キャッシュと差分があれば `keymap-updated` イベントで差し替え。セッション中 1 回成功したら再取得しない(`KeymapState`)
  - 画像は data URL(base64)で IPC 渡し(4 枚計 ~530KB。asset protocol 不要)。PNG マジックバイト検証で captive portal 等の偽応答をキャッシュしない
  - reqwest は rustls-tls(cargo-xwin クロスビルドで OpenSSL を避けるため必須)
  - main ウィンドウは `visible: false` で起動時に作成されるため、Webview の事前ロードで初回取得も起動直後に走る。閉じても hide(CloseRequested ハンドラ)
- **ライブキーマップ(v2、マイルストーン 4 拡張・アプリ側実装済み)**: `studio/` モジュール。実機から取得できたら PopupView が画像の代わりに SVG 描画へ切り替える。このレイヤーを呼び出すキー(hold = レイヤー名)はアクセントでハイライト
  - GATT: service `00000000-0196-6107-c967-c5cfb1c2482a`、RPC characteristic `00000001-…`(write + **indicate**。bluest の `notify()` は properties を見て indicate を自動選択する — 確認済み)
  - フレーミングは SOF/ESC/EOF = 0xAB/0xAC/0xAD(zmk の msg_framing.c と対称実装。framing.rs にテストあり)
  - proto は zmk-studio-messages @ `6cb4c28`(zmk **v0.3** の west.yml が参照する版)を `src-tauri/proto/` に vendored し、prost + protoc-bin-vendored でビルド時生成(システム protoc 不要)
  - binding → ラベルは behavior メタデータのパラメータ型(HidUsage / LayerId / Constant / Range / Nil)で汎用解釈し、&kp/&mo/&lt/&mt/&bt を個別実装しない。2 パラメータは param1=hold / param2=tap。HID usage は keyboard/consumer page の主要どころ+LANG1/2(かな/英数)。Shift 単独修飾は US shifted 記号に変換
  - `get_keymap` / `get_physical_layouts` / `get_behavior_details` は **SECURED**(locked だと UNLOCK_REQUIRED)。`get_lock_state` を先に呼んで locked なら分かりやすく報告する
  - 常時接続はしない: ポップアップ(main)の Webview ロード時に 1 回取得し、初回が「未接続」で終わった場合は `battery-updated` の接続完了を拾って自動再取得。battery 層と独立に、接続済みデバイスを Studio service UUID で列挙して探す — 見つからなければ「ファーム未対応」と判定して画像へフォールバック
- ウィンドウを閉じても hide のみで常駐継続。終了はトレイメニュー「KobitoBar を終了」→ `app.exit(0)`。`RunEvent::ExitRequested` は `code.is_none()` のときだけ `prevent_exit()`
- macOS では `ActivationPolicy::Accessory` で Dock 非表示
- フロント ⇔ Rust の IPC 契約: `get_battery_status` / `get_keymap_images` / `get_live_keymap` コマンド+ `battery-updated` / `keymap-updated` イベント(フロントは受信時刻をフッターに「↻ HH:MM」表示、未接続時は「last HH:MM」)
- 現在のアイコンは `scripts/generate-icon.mjs` によるプレースホルダの小人ドット絵。マイルストーン 5 で本番アートに差し替える

## 開発環境の注意(このマシン = WSL2)

- WSL2 には Bluetooth スタックがないため、WSL 内での BLE 実機確認は不可
- webkit2gtk-4.1 / GTK3 はインストール済みで、**このマシンで `pnpm build` と `cargo check` が通ることは確認済み**(2026-08-04)
- **UI 開発は WSLg で可能**: `pnpm tauri dev` で Windows デスクトップ上にウィンドウが表示される(確認済み 2026-08-04)。ただし WSLg にはトレイ表示ホストがなく**トレイアイコンは出ない**。このため debug ビルドは起動時にポップアップを自動表示する(`lib.rs` の `#[cfg(debug_assertions)]`)。libEGL / MESA / Gtk-CRITICAL 警告は WSLg のソフトウェアレンダリング関連で無害
- **BLE・トレイ込みの実機確認は `scripts/dev-windows.sh`**(確認済み 2026-08-04): cargo-xwin で Windows 用 .exe をクロスビルド → Windows 側 `%TEMP%\KobitoBar` にコピー → WSL interop で起動。Windows の実 Bluetooth スタック・実トレイで動く。ホットリロードはなし。cargo-xwin と x86_64-pc-windows-msvc target はインストール済み
- bluest は Linux backend(BlueZ / bluer)を持つため**無条件依存**にしてある。WSL2 でも BLE コード込みで `cargo check`・`cargo test` が通る(実行時は `Adapter::default()` が None になり 30 秒間隔で待機し続けるだけで壊れない)。`cargo check` に bluer v0.16 の future-incompat 警告が出るが依存側の問題で実害なし
