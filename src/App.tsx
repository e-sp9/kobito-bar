import PopupView from "./PopupView";

// 単一ウィンドウ構成。キーマップはトレイポップアップ内に統合されている
// (docs/claude-design-prompt.md「別ウィンドウへ逃さない」)
export default function App() {
  return <PopupView />;
}
