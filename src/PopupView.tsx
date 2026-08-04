import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

// src-tauri/src/battery/mod.rs の BatteryStatus と対応する IPC 契約
type BatteryStatus = {
  connected: boolean;
  left: number | null;
  right: number | null;
};

// src-tauri/src/keymap.rs の KeymapImage と対応する IPC 契約(静的画像)
type KeymapImage = {
  layer: number;
  name: string;
  dataUrl: string;
  source: "network" | "cache" | "bundled";
};

// src-tauri/src/studio/mod.rs と対応する IPC 契約(実機から取得)
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

function formatTime(date: Date): string {
  return date.toLocaleTimeString("ja-JP", { hour: "2-digit", minute: "2-digit" });
}

function fallbackReason(kind: string): string {
  switch (kind) {
    case "notSupported":
      return "studio disabled in firmware";
    case "locked":
      return "studio locked";
    default:
      return "device not responding";
  }
}

// ラベル長に応じたフォントサイズ(単位: u)。デザインの 25px キーでの
// 10 / 8.5 / 7px 相当(1u ≈ 28px)
function tapFontSize(label: string): number {
  const len = [...label].length;
  if (len <= 1) return 0.36;
  if (len <= 3) return 0.3;
  if (len <= 5) return 0.24;
  return 0.19;
}

// ポップアップ内のミニキーマップ。実機データ(キーピッチ単位)を
// そのまま viewBox にして 142px の枠へ収める
function MiniKeymap({ keymap, layer }: { keymap: LiveKeymap; layer: number }) {
  const pad = 0.25;
  const minX = Math.min(...keymap.keys.map((k) => k.x));
  const minY = Math.min(...keymap.keys.map((k) => k.y));
  const maxX = Math.max(...keymap.keys.map((k) => k.x + k.width));
  const maxY = Math.max(...keymap.keys.map((k) => k.y + k.height));
  const bindings = keymap.layers[layer]?.bindings ?? [];
  const layerName = keymap.layers[layer]?.name;
  const inset = 0.05;

  return (
    <svg
      className="mini-keymap"
      viewBox={`${minX - pad} ${minY - pad} ${maxX - minX + pad * 2} ${maxY - minY + pad * 2}`}
    >
      {keymap.keys.map((k, i) => {
        const label = bindings[i] ?? { tap: "", hold: null };
        const blank = label.tap === "" || label.tap === "▽";
        // このレイヤーを呼び出すキー(hold がレイヤー名)はアクセント表示
        const active =
          layer > 0 &&
          (label.hold === layerName || (!label.hold && label.tap === layerName));
        const state = active ? " active" : blank ? " blank" : "";
        const cx = k.x + k.width / 2;
        return (
          <g
            key={i}
            transform={k.r !== 0 ? `rotate(${k.r} ${k.rx} ${k.ry})` : undefined}
          >
            <rect
              className={`mini-key${state}`}
              x={k.x + inset}
              y={k.y + inset}
              width={k.width - inset * 2}
              height={k.height - inset * 2}
              rx={0.12}
            />
            <text
              className={`mini-tap${state}`}
              x={cx}
              y={k.y + k.height * (label.hold ? 0.38 : 0.5)}
              fontSize={tapFontSize(label.tap)}
            >
              {label.tap}
            </text>
            {label.hold && (
              <text
                className={`mini-hold${active ? " active" : ""}`}
                x={cx}
                y={k.y + k.height * 0.74}
                fontSize={0.23}
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

function BatterySide({
  letter,
  level,
  connected,
}: {
  letter: string;
  level: number | null;
  connected: boolean;
}) {
  const low = level != null && level <= 20;
  // 低残量は色以外の手掛かり(チップの形と文言)も併用する
  const chip = low
    ? { text: "! low", warn: true }
    : level == null && connected
      ? { text: "○ not linked", warn: false }
      : null;
  const filled = level == null ? 0 : Math.round(level / 10);

  return (
    <div className="side">
      <div className="side-id">
        <span className="side-letter">{letter}</span>
        {chip && (
          <span className={`side-chip${chip.warn ? " warn" : ""}`}>
            {chip.text}
          </span>
        )}
      </div>
      <div
        className="side-num"
        data-state={level == null ? "unknown" : low ? "low" : "ok"}
      >
        <span className="num">{level ?? "--"}</span>
        {level != null && <span className="pct">%</span>}
      </div>
      <div className="side-gauge">
        {Array.from({ length: 10 }, (_, i) => (
          <span
            key={i}
            className={`seg${i < filled ? (low ? " on warn" : " on") : ""}`}
          />
        ))}
      </div>
    </div>
  );
}

export default function PopupView() {
  const [status, setStatus] = useState<BatteryStatus>({
    connected: false,
    left: null,
    right: null,
  });
  const [updatedAt, setUpdatedAt] = useState<Date | null>(null);
  const [images, setImages] = useState<KeymapImage[]>([]);
  // null = 実機から取得中
  const [live, setLive] = useState<LiveKeymapResult | null>(null);
  const [layer, setLayer] = useState(0);
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
    const apply = (next: BatteryStatus) => {
      setStatus(next);
      if (next.connected) setUpdatedAt(new Date());
    };
    invoke<BatteryStatus>("get_battery_status").then(apply).catch(console.error);
    const unlistenBattery = listen<BatteryStatus>("battery-updated", (event) => {
      apply(event.payload);
      // このビューは起動時にロードされるため、初回の実機取得は BLE 接続前に
      // 走って「未接続」で終わることが多い。接続完了を拾って自動でやり直す
      const prev = liveRef.current;
      if (
        event.payload.connected &&
        prev?.status === "unavailable" &&
        prev.kind === "notConnected"
      ) {
        fetchLive();
      }
    });
    invoke<KeymapImage[]>("get_keymap_images")
      .then(setImages)
      .catch(console.error);
    // バックグラウンド更新(GitHub raw)が完了したら最新に差し替える
    const unlistenKeymap = listen<KeymapImage[]>("keymap-updated", (event) => {
      setImages(event.payload);
    });
    fetchLive();
    return () => {
      unlistenBattery.then((fn) => fn());
      unlistenKeymap.then((fn) => fn());
    };
  }, []);

  const liveKeymap = live?.status === "ready" ? live.keymap : null;
  // scan = 完全未接続 / live = 実機表示 / loading = 実機から取得中 /
  // fallback = 取得できず静的画像(理由 1 行つき)
  const mode = !status.connected
    ? "scan"
    : liveKeymap
      ? "live"
      : live === null
        ? "loading"
        : "fallback";

  const tabs = liveKeymap
    ? liveKeymap.layers.map((l) => l.name)
    : images.map((img) => img.name);
  const activeLayer = Math.min(layer, Math.max(tabs.length - 1, 0));
  const currentImage = images[activeLayer];

  const time = updatedAt ? formatTime(updatedAt) : null;
  const footTime = status.connected
    ? `↻ ${time ?? "--:--"}`
    : time
      ? `last ${time}`
      : "—";
  const footSrc = { live: "keymap:live", loading: "keymap:…", fallback: "keymap:static", scan: "keymap:—" }[mode];
  const foot = !status.connected
    ? { dot: "○", cls: "off", text: "scanning…" }
    : status.right == null
      ? { dot: "◐", cls: "half", text: "right not linked" }
      : status.left == null
        ? { dot: "◐", cls: "half", text: "syncing…" }
        : { dot: "●", cls: "on", text: "connected" };

  // Windows はタスクバーから上向きに開くためカードを下端へ寄せる。
  // macOS はメニューバーから下向きなので上端へ
  const isMac = navigator.userAgent.includes("Macintosh");

  return (
    <main className={`popup-frame${isMac ? " anchor-top" : ""}`}>
      <div className="card">
        <span className="corner tl" />
        <span className="corner tr" />
        <span className="corner bl" />
        <span className="corner br" />

        <header className="term-head">
          <span>❯ kobitobar</span>
          <span className="term-meta">
            {status.connected ? "ble·kobitokey" : "ble·—"}
          </span>
        </header>

        <div className="sect sect-keymap">
          <span>keymap</span>
          <span className="rule" />
          <span className="diamond" />
        </div>

        {live?.status === "unavailable" && mode === "fallback" && (
          <div className="fb-note" title={live.detail}>
            <span className="fb-icon">!</span>
            <span>{fallbackReason(live.kind)} — showing static image</span>
          </div>
        )}

        {mode !== "scan" && tabs.length > 0 && (
          <nav className="tabs">
            {tabs.map((name, i) => (
              <button
                key={i}
                className={i === activeLayer ? "active" : ""}
                onClick={() => setLayer(i)}
              >
                {i}:{name.toLowerCase()}
              </button>
            ))}
          </nav>
        )}

        {mode === "scan" ? (
          <div className="map map-scan">
            <span className="scan-dot">○</span>
            <span className="scan-msg">scanning for kobitokey…</span>
            <span className="scan-hint">power on the keyboard to connect</span>
          </div>
        ) : mode === "live" && liveKeymap ? (
          <div className="map map-live" key={`live-${activeLayer}`}>
            <MiniKeymap keymap={liveKeymap} layer={activeLayer} />
          </div>
        ) : (
          <div className="map map-static" key={`static-${activeLayer}`}>
            {currentImage ? (
              <img
                src={currentImage.dataUrl}
                alt={`Layer ${currentImage.layer}: ${currentImage.name}`}
              />
            ) : (
              <>
                <span className="static-name">images/layer{activeLayer}.png</span>
                <span className="static-cap">static png · bundled with app</span>
              </>
            )}
          </div>
        )}

        <div className="sect sect-battery">
          <span>battery</span>
          <span className="rule" />
          <span className="diamond" />
        </div>

        <div className="sides">
          <BatterySide letter="L" level={status.left} connected={status.connected} />
          <BatterySide letter="R" level={status.right} connected={status.connected} />
        </div>

        <footer className="foot">
          <span className="foot-status">
            <span className={`dot ${foot.cls}`}>{foot.dot}</span>
            <span>{foot.text}</span>
          </span>
          <span className="foot-dim">{footTime}</span>
          <span className="foot-dim">{footSrc}</span>
        </footer>
      </div>
    </main>
  );
}
