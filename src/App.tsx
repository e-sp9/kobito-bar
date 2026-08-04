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

function formatTime(date: Date): string {
  return date.toLocaleTimeString("ja-JP", { hour: "2-digit", minute: "2-digit" });
}

export default function App() {
  const [status, setStatus] = useState<BatteryStatus>({
    connected: false,
    left: null,
    right: null,
  });
  const [updatedAt, setUpdatedAt] = useState<Date | null>(null);

  useEffect(() => {
    const apply = (next: BatteryStatus) => {
      setStatus(next);
      if (next.connected) setUpdatedAt(new Date());
    };
    invoke<BatteryStatus>("get_battery_status").then(apply).catch(console.error);
    const unlisten = listen<BatteryStatus>("battery-updated", (event) => {
      apply(event.payload);
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
      <footer className="popup-footer">
        {status.connected
          ? updatedAt && `${formatTime(updatedAt)} 更新`
          : "KobitoKey を探しています…"}
      </footer>
    </main>
  );
}
