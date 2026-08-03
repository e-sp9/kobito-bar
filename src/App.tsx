import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

// src-tauri/src/lib.rs の BatteryStatus と対応する IPC 契約
type BatteryStatus = {
  connected: boolean;
  left: number | null;
  right: number | null;
};

function BatteryRow({ label, level }: { label: string; level: number | null }) {
  const state = level == null ? "unknown" : level <= 20 ? "low" : "ok";
  return (
    <div className="battery-row">
      <span className="battery-label">{label}</span>
      <div className="battery-gauge">
        <div
          className="battery-fill"
          data-state={state}
          style={{ width: `${level ?? 0}%` }}
        />
      </div>
      <span className="battery-value">{level == null ? "--%" : `${level}%`}</span>
    </div>
  );
}

export default function App() {
  const [status, setStatus] = useState<BatteryStatus>({
    connected: false,
    left: null,
    right: null,
  });

  useEffect(() => {
    invoke<BatteryStatus>("get_battery_status").then(setStatus).catch(console.error);
    const unlisten = listen<BatteryStatus>("battery-updated", (event) => {
      setStatus(event.payload);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  return (
    <main className="popup">
      <header className="popup-header">
        <h1>KobitoBar</h1>
        <span className={`connection ${status.connected ? "online" : "offline"}`}>
          {status.connected ? "接続中" : "未接続"}
        </span>
      </header>
      <div className="kobito-stage">
        <span className="kobito-placeholder">🔨</span>
        <p>こびとたちは準備中…</p>
      </div>
      <div className="battery-list">
        <BatteryRow label="左手" level={status.left} />
        <BatteryRow label="右手" level={status.right} />
      </div>
    </main>
  );
}
