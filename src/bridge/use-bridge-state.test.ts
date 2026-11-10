// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only
import { expect, it } from "vitest";
import { INITIAL_BRIDGE_SNAPSHOT, mergeSnapshot } from "./use-bridge-state";
import type { ActionAttempt } from "./types";

it("merges bounded activity deltas without dropping prior entries", () => {
  const action: ActionAttempt = {
    action: "ID_TEST",
    requestId: "r1",
    label: null,
    platform: "linux",
    shortcut: null,
    validation: "rejected",
    validationMessage: "unknown",
    executed: false,
    receivedAt: "now",
    sequence: 1,
  };
  const first = mergeSnapshot(INITIAL_BRIDGE_SNAPSHOT, {
    ...INITIAL_BRIDGE_SNAPSHOT,
    actionHistory: undefined,
    revision: 1,
    lastAction: action,
  });
  const second = mergeSnapshot(first, {
    ...first,
    actionHistory: undefined,
    revision: 2,
    lastAction: { ...action, sequence: 2 },
  });
  expect(second.actionHistory.map((item) => item.sequence)).toEqual([1, 2]);
  expect(mergeSnapshot(second, { ...first, actionHistory: undefined })).toBe(
    second,
  );
  expect(
    mergeSnapshot(second, { ...second, actionHistory: undefined })
      .actionHistory,
  ).toHaveLength(2);
  const recovered = mergeSnapshot(second, {
    ...second,
    revision: 3,
    actionHistory: [{ ...action, sequence: 3 }],
  });
  expect(recovered.actionHistory.map((item) => item.sequence)).toEqual([3]);
});

it("fills a missed action from a recovery response that arrives after a newer event", () => {
  const action: ActionAttempt = {
    action: "ID_TEST",
    requestId: "r1",
    label: null,
    platform: "linux",
    shortcut: null,
    validation: "rejected",
    validationMessage: "unknown",
    executed: false,
    receivedAt: "now",
    sequence: 1,
  };
  const current = {
    ...INITIAL_BRIDGE_SNAPSHOT,
    revision: 4,
    serverStatus: "active" as const,
    lastAction: { ...action, sequence: 3 },
    actionHistory: [{ ...action, sequence: 3 }],
  };
  const recovered = mergeSnapshot(current, {
    ...INITIAL_BRIDGE_SNAPSHOT,
    revision: 3,
    lastAction: { ...action, sequence: 2 },
    actionHistory: [action, { ...action, sequence: 2 }],
  });
  expect(recovered.revision).toBe(4);
  expect(recovered.serverStatus).toBe("active");
  expect(recovered.actionHistory.map((item) => item.sequence)).toEqual([
    1, 2, 3,
  ]);
});
