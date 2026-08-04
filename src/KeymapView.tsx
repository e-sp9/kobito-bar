import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

// src-tauri/src/keymap.rs の KeymapImage と対応する IPC 契約(v1: 画像)
type KeymapImage = {
  layer: number;
  name: string;
  dataUrl: string;
  source: "network" | "cache" | "bundled";
};

// src-tauri/src/studio/mod.rs と対応する IPC 契約(v2: 実機から取得)
type KeyLabel = { tap: string; hold: string | null };
type KeyShape = {
  x: number;
  y: number;
  width: number;
  height: number;
  r: number;
  rx: number;
  ry: number;
};
type LiveKeymap = {
  layoutName: string;
  keys: KeyShape[];
  layers: { id: number; name: string; bindings: KeyLabel[] }[];
};
type LiveKeymapResult =
  | { status: "ready"; keymap: LiveKeymap }
  | {
      status: "unavailable";
      kind: "notConnected" | "notSupported" | "locked" | "error";
      detail: string;
    };

function unavailableMessage(kind: string): string {
  switch (kind) {
    case "notConnected":
      return "KobitoKey 未接続のため、リポジトリのキーマップ画像を表示しています";
    case "notSupported":
      return "ファームウェアの ZMK Studio が無効のため、キーマップ画像を表示しています";
    case "locked":
      return "キーボードが Studio ロック中のため、キーマップ画像を表示しています";
    default:
      return "実機から取得できなかったため、キーマップ画像を表示しています";
  }
}

// ラベル長に応じてキーキャップ内に収まるフォントサイズ(単位: u)を返す
function labelFontSize(label: string): number {
  const len = [...label].length;
  if (len <= 1) return 0.3;
  if (len === 2) return 0.26;
  if (len === 3) return 0.2;
  if (len <= 5) return 0.15;
  return 0.12;
}

function LiveKeymapSvg({ keymap, layer }: { keymap: LiveKeymap; layer: number }) {
  const pad = 0.25;
  const minX = Math.min(...keymap.keys.map((k) => k.x));
  const minY = Math.min(...keymap.keys.map((k) => k.y));
  const maxX = Math.max(...keymap.keys.map((k) => k.x + k.width));
  const maxY = Math.max(...keymap.keys.map((k) => k.y + k.height));
  const bindings = keymap.layers[layer]?.bindings ?? [];
  const inset = 0.04;

  return (
    <svg
      className="live-keymap"
      viewBox={`${minX - pad} ${minY - pad} ${maxX - minX + pad * 2} ${maxY - minY + pad * 2}`}
    >
      {keymap.keys.map((k, i) => {
        const label = bindings[i] ?? { tap: "", hold: null };
        const cx = k.x + k.width / 2;
        return (
          <g
            key={i}
            transform={k.r !== 0 ? `rotate(${k.r} ${k.rx} ${k.ry})` : undefined}
          >
            <rect
              className="live-key"
              x={k.x + inset}
              y={k.y + inset}
              width={k.width - inset * 2}
              height={k.height - inset * 2}
              rx={0.08}
            />
            <text
              className="live-key-tap"
              x={cx}
              y={k.y + k.height / 2}
              fontSize={labelFontSize(label.tap)}
            >
              {label.tap}
            </text>
            {label.hold && (
              <text
                className="live-key-hold"
                x={cx}
                y={k.y + k.height - 0.16}
                fontSize={0.13}
              >
                {label.hold}
              </text>
            )}
          </g>
        );
      })}
    </svg>
  );
}

export default function KeymapView() {
  const [images, setImages] = useState<KeymapImage[]>([]);
  const [error, setError] = useState<string | null>(null);
  // null = 実機から取得中
  const [live, setLive] = useState<LiveKeymapResult | null>(null);
  const [active, setActive] = useState(0);
  const liveRef = useRef(live);
  liveRef.current = live;

  const fetchLive = () => {
    setLive(null);
    invoke<LiveKeymapResult>("get_live_keymap")
      .then(setLive)
      .catch((e) =>
        setLive({ status: "unavailable", kind: "error", detail: String(e) }),
      );
  };

  useEffect(() => {
    invoke<KeymapImage[]>("get_keymap_images")
      .then(setImages)
      .catch((e) => setError(String(e)));
    // バックグラウンド更新(GitHub raw)が完了したら最新に差し替える
    const unlisten = listen<KeymapImage[]>("keymap-updated", (event) => {
      setImages(event.payload);
    });
    // このビューは起動時に事前ロードされるため、初回の実機取得は BLE 接続前に
    // 走って「未接続」で終わることが多い。接続完了を拾って自動でやり直す
    const unlistenBattery = listen<{ connected: boolean }>(
      "battery-updated",
      (event) => {
        const prev = liveRef.current;
        if (
          event.payload.connected &&
          prev?.status === "unavailable" &&
          prev.kind === "notConnected"
        ) {
          fetchLive();
        }
      },
    );
    fetchLive();
    return () => {
      unlisten.then((fn) => fn());
      unlistenBattery.then((fn) => fn());
    };
  }, []);

  const liveKeymap = live?.status === "ready" ? live.keymap : null;

  if (error && !liveKeymap) {
    return (
      <main className="keymap-status">
        キーマップを読み込めませんでした: {error}
      </main>
    );
  }

  // タブはライブ取得できたら実機のレイヤー名、それまでは画像側の定義
  const tabs = liveKeymap
    ? liveKeymap.layers.map((l, i) => ({ no: i, name: l.name }))
    : images.map((img) => ({ no: img.layer, name: img.name }));

  if (tabs.length === 0) {
    return <main className="keymap-status">読み込み中…</main>;
  }

  const activeIndex = Math.min(active, tabs.length - 1);
  const currentImage = images[activeIndex];

  return (
    <main className="keymap">
      <nav className="keymap-tabs">
        {tabs.map((tab, i) => (
          <button
            key={tab.no}
            className={i === activeIndex ? "active" : ""}
            onClick={() => setActive(i)}
          >
            <span className="layer-no">Layer {tab.no}</span>
            {tab.name}
          </button>
        ))}
      </nav>
      <div className="keymap-canvas">
        {liveKeymap ? (
          <LiveKeymapSvg keymap={liveKeymap} layer={activeIndex} />
        ) : currentImage ? (
          <img
            src={currentImage.dataUrl}
            alt={`Layer ${currentImage.layer}: ${currentImage.name}`}
          />
        ) : null}
      </div>
      {!liveKeymap && currentImage?.source === "bundled" && (
        <p className="keymap-note">
          オフラインのためアプリ同梱のキーマップを表示しています(最新でない可能性があります)
        </p>
      )}
      <footer className="keymap-statusbar">
        {live === null ? (
          <span>実機からキーマップを取得中…</span>
        ) : live.status === "ready" ? (
          <span className="live-badge">
            実機のキーマップを表示中(レイアウト: {live.keymap.layoutName})
          </span>
        ) : (
          <span title={live.detail}>{unavailableMessage(live.kind)}</span>
        )}
        {live !== null && (
          <button className="reload" onClick={fetchLive}>
            再読込
          </button>
        )}
      </footer>
    </main>
  );
}
