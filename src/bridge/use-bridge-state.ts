// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";

import type { BridgeSnapshot } from "./types";

export const INITIAL_BRIDGE_SNAPSHOT: BridgeSnapshot = {
  revision: 0,
  applicationStatus: "running",
  serverStatus: "stopped",
  endpoint: "http://0.0.0.0:53177",
  serverError: null,
  allowedOrigins: ["http://localhost:5173", "http://127.0.0.1:5173"],
  lastMessage: null,
  lastAction: null,
  actionHistory: [],
  activePairing: null,
  devices: [],
  activeConnectionCount: 0,
};

export function useBridgeState(): BridgeSnapshot {
  const [snapshot, setSnapshot] = useState<BridgeSnapshot>(
    INITIAL_BRIDGE_SNAPSHOT,
  );

  useEffect(() => {
    let disposed = false;
    let unlisten: UnlistenFn | undefined;

    const applySnapshot = (incoming: BridgeSnapshot) => {
      if (disposed) return;
      setSnapshot((current) =>
        incoming.revision >= current.revision ? incoming : current,
      );
    };

    const synchronize = async () => {
      try {
        const stopListening = await listen<BridgeSnapshot>(
          "bridge-state-changed",
          (event) => applySnapshot(event.payload),
        );

        if (disposed) {
          stopListening();
          return;
        }

        unlisten = stopListening;
        applySnapshot(await invoke<BridgeSnapshot>("get_bridge_snapshot"));
      } catch (error) {
        if (disposed) return;
        const detail = error instanceof Error ? error.message : String(error);
        setSnapshot((current) => ({
          ...current,
          serverStatus: "error",
          serverError: `Unable to read companion state: ${detail}`,
        }));
      }
    };

    void synchronize();

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  return snapshot;
}
