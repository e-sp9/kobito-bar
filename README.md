# KobitoBar

自作分割キーボード [KobitoKey](https://github.com/e-sp9/KobitoKey_QWERTY) のコンパニオン常駐アプリ。
メニューバー(macOS)/ システムトレイ(Windows)に小人が住みつき、キーボードの様子を教えてくれます。

## 機能(開発中)

- [x] トレイ常駐・ログイン時自動起動
- [ ] 左右それぞれの電池残量表示(BLE)
- [ ] 低残量時の通知
- [ ] キーマップ一覧表示(レイヤーごとのキー配置図)
- [ ] 電池残量に連動する小人のドット絵アニメーション(働く → 休む → 寝る)

## 開発

前提: Node.js / pnpm / Rust([Tauri v2 の前提条件](https://v2.tauri.app/start/prerequisites/)を参照)

```sh
pnpm install
pnpm tauri dev    # 開発起動
pnpm tauri build  # 配布ビルド
```

アイコンの再生成:

```sh
node scripts/generate-icon.mjs && pnpm tauri icon
```
