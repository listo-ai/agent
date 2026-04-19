import { useState, version as reactVersion } from "react";
import { create } from "zustand";

// Small local Zustand store — proves the remote can create its own stores
// while still sharing the zustand runtime singleton with the host.
const useCounterStore = create<{ count: number; inc: () => void }>((set) => ({
  count: 0,
  inc: () => set((s) => ({ count: s.count + 1 })),
}));

// Expose React version at module scope so the host can verify singleton-ness:
// host and remote printing the same string is the proof.
export const REMOTE_REACT_VERSION = reactVersion;

export default function Panel() {
  const [local, setLocal] = useState("hello from plugin_hello");
  const count = useCounterStore((s) => s.count);
  const inc = useCounterStore((s) => s.inc);

  return (
    <div
      style={{
        padding: 16,
        border: "1px solid #888",
        borderRadius: 8,
        fontFamily: "system-ui, sans-serif",
      }}
    >
      <h3 style={{ margin: 0, fontSize: 14 }}>Remote plugin (MF)</h3>
      <p style={{ fontSize: 12, opacity: 0.7 }}>
        React version seen by remote: <code>{reactVersion}</code>
      </p>

      <div style={{ display: "flex", gap: 8, alignItems: "center", marginTop: 8 }}>
        <button onClick={inc} style={{ padding: "4px 10px" }}>
          Remote counter: {count}
        </button>
        <input
          value={local}
          onChange={(e) => setLocal(e.target.value)}
          style={{ flex: 1, padding: 4 }}
        />
      </div>
    </div>
  );
}
