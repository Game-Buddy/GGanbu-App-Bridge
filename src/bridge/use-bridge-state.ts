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

type SnapshotUpdate = Omit<BridgeSnapshot, "actionHistory"> & {
  actionHistory?: BridgeSnapshot["actionHistory"];
};

export function mergeSnapshot(
  current: BridgeSnapshot,
  incoming: SnapshotUpdate,
): BridgeSnapshot {
  if (incoming.revision < current.revision) {
    if (!incoming.actionHistory?.length) return current;
    // A recovery response may arrive after another event; fill history without rolling back state.
    const entries = new Map(
      [...incoming.actionHistory, ...current.actionHistory].map((attempt) => [
        attempt.sequence,
        attempt,
      ]),
    );
    const history = [...entries.values()]
      .sort((a, b) => a.sequence - b.sequence)
      .slice(-200);
    if (
      history.length === current.actionHistory.length &&
      history.every(
        (attempt, index) =>
          attempt.sequence === current.actionHistory[index]?.sequence,
      )
    )
      return current;
    return { ...current, actionHistory: history };
  }
  const lastAction = incoming.lastAction;
  const actionHistory =
    incoming.actionHistory ??
    (lastAction && lastAction.sequence > (current.lastAction?.sequence ?? 0)
      ? [...current.actionHistory, lastAction].slice(-200)
      : current.actionHistory);
  return { ...incoming, actionHistory };
}

export function useBridgeState(): BridgeSnapshot {
  const [snapshot, setSnapshot] = useState<BridgeSnapshot>(
    INITIAL_BRIDGE_SNAPSHOT,
  );

  useEffect(() => {
    let disposed = false;
    let lastRevision = 0;
    let unlisten: UnlistenFn | undefined;

    const applySnapshot = (incoming: SnapshotUpdate) => {
      if (disposed) return;
      setSnapshot((current) => mergeSnapshot(current, incoming));
    };

    const synchronize = async () => {
      try {
        const stopListening = await listen<SnapshotUpdate>(
          "bridge-state-changed",
          (event) => {
            applySnapshot(event.payload);
            // Recover missed/reordered activity events from the authoritative bounded history.
            if (event.payload.actionHistory === undefined) {
              const revision = event.payload.revision;
              if (revision > lastRevision + 1) {
                void invoke<BridgeSnapshot>("get_bridge_snapshot")
                  .then(applySnapshot)
                  .catch(console.error);
              }
            }
            lastRevision = Math.max(lastRevision, event.payload.revision);
          },
        );

        if (disposed) {
          stopListening();
          return;
        }

        unlisten = stopListening;
        const initial = await invoke<BridgeSnapshot>("get_bridge_snapshot");
        applySnapshot(initial);
        lastRevision = Math.max(lastRevision, initial.revision);
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
