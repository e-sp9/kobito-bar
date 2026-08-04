# ZMK Studio RPC proto 定義(vendored)

- 出典: https://github.com/zmkfirmware/zmk-studio-messages
- revision: `6cb4c283e76209d59c45fbcb218800cd19e9339d`
  (zmk **v0.3** の `app/west.yml` が参照している版。KobitoKey_QWERTY は zmk v0.3)

`build.rs` が prost-build でコンパイルし、`src/studio/proto.rs` が include する。

KobitoKey_QWERTY 側で zmk のバージョンを上げたときは、その版の
`app/west.yml` にある zmk-studio-messages の revision を確認し、
差分があればここも更新すること。
