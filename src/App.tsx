import KeymapView from "./KeymapView";
import PopupView from "./PopupView";

// ウィンドウごとの表示切替。keymap ウィンドウは index.html?view=keymap で開く
// (tauri.conf.json の windows[].url を参照)
export default function App() {
  const view = new URLSearchParams(window.location.search).get("view");
  return view === "keymap" ? <KeymapView /> : <PopupView />;
}
