// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

export type ServerStatus = "starting" | "stopped" | "active" | "error";

export interface ReceivedMessage {
  text: string;
  receivedAt: string;
  sequence: number;
}

export type TargetPlatform = "linux" | "windows" | "macos" | "unsupported";
export type ShortcutModifier = "control" | "alt" | "shift" | "meta";
export type ActionValidation = "accepted" | "rejected";

export interface ResolvedShortcut {
  keyCodes: number[];
  modifiers: ShortcutModifier[];
  keys: string[];
  display: string;
}

export interface ActionAttempt {
  action: string;
  requestId: string;
  label: string | null;
  platform: TargetPlatform;
  shortcut: ResolvedShortcut | null;
  validation: ActionValidation;
  validationMessage: string;
  executed: boolean;
  receivedAt: string;
  sequence: number;
}

export interface PairingDisplay {
  code: string;
  expiresAt: string;
  failedAttempts: number;
}

export type DeviceConnectionStatus = "connected" | "idle";

export interface DeviceSummary {
  id: string;
  name: string;
  status: DeviceConnectionStatus;
  firstPaired: string;
  lastSeen: string;
  sessionCount: number;
}

export interface BridgeSnapshot {
  revision: number;
  applicationStatus: "running";
  serverStatus: ServerStatus;
  endpoint: "http://0.0.0.0:53177";
  serverError: string | null;
  allowedOrigins: string[];
  lastMessage: ReceivedMessage | null;
  lastAction: ActionAttempt | null;
  actionHistory: ActionAttempt[];
  activePairing: PairingDisplay | null;
  devices: DeviceSummary[];
  activeConnectionCount: number;
}
