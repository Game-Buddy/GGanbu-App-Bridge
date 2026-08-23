// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

import { invoke } from "@tauri-apps/api/core";
import { useEffect, useId, useState } from "react";
import type { ReactNode } from "react";
import { createPortal } from "react-dom";
import type { ServerStatus } from "./bridge/types";
import { useBridgeState } from "./bridge/use-bridge-state";
import appImage from "./assets/favicon.svg";
import { APP_VERSION } from "./version";

const RELEASE_TAG = `v${APP_VERSION}`;
const LICENSE_URL = `https://github.com/Game-Buddy/GGanbu-App-Bridge/blob/${RELEASE_TAG}/LICENSE`;
const SOURCE_URL = `https://github.com/Game-Buddy/GGanbu-App-Bridge/tree/${RELEASE_TAG}`;
const DEFAULT_KEYBINDINGS_FILENAME = "hotkey.keyboard_shooter_ver3.blkx";

const SERVER_LABELS: Record<ServerStatus, string> = {
  starting: "Starting",
  stopped: "Not Running",
  active: "Active",
  error: "Error",
};

const NAV_ITEMS = [
  { label: "Status", icon: "status" },
  { label: "Connections", icon: "connections" },
  { label: "Activity", icon: "activity" },
  { label: "Mappings", icon: "mappings" },
  { label: "Help", icon: "help" },
  { label: "About", icon: "about" },
] as const;

type IconName = (typeof NAV_ITEMS)[number]["icon"];
type KeybindingRow = { action: string; binding: string };
type KeybindingPreview = {
  source: string;
  output: string;
  files: string[];
  bindings: number;
  hotkeys: KeybindingRow[];
};

function Icon({ name }: { name: IconName }) {
  const paths: Record<IconName, string> = {
    status: "M12 3.5a8.5 8.5 0 1 0 8.5 8.5M12 7.5v4.8l3 1.8",
    activity: "M3.5 18.5 8 13l3.2 3.2L17 8.5l3.5 3.5M3.5 20.5h17",
    connections:
      "M8.8 15.2 7.2 16.8a3.6 3.6 0 0 1-5-5l2.8-2.8a3.6 3.6 0 0 1 5 0M15.2 8.8l1.6-1.6a3.6 3.6 0 0 1 5 5L19 15a3.6 3.6 0 0 1-5 0M7.5 16.5l9-9",
    mappings: "M4 6h16M4 12h16M4 18h16M8 4v4m8 2v4m-5 6v-4M6 6h4m5 6h4m-7 6h4",
    about: "M12 16v.01M12 12V8M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Z",
    help: "M12 17v.01M9.5 9a2.5 2.5 0 1 1 4.6 1.4c-.8 1-2.1 1.2-2.1 2.6M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Z",
  };

  return (
    <svg aria-hidden="true" className="icon" viewBox="0 0 24 24" fill="none">
      <path
        d={paths[name]}
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.6"
      />
    </svg>
  );
}

interface ActivityCellProps {
  children?: ReactNode;
  value: string;
}

interface ActivityPopoverPosition {
  above: boolean;
  left: number;
  top: number;
}

function ActivityCell({ children, value }: ActivityCellProps) {
  const tooltipId = useId();
  const [popover, setPopover] = useState<ActivityPopoverPosition | null>(null);

  const showPopover = (cell: HTMLTableCellElement) => {
    const rect = cell.getBoundingClientRect();
    const popoverWidth = Math.min(320, window.innerWidth - 16);
    const left = Math.max(
      8,
      Math.min(rect.left, window.innerWidth - popoverWidth - 8),
    );
    const above = rect.bottom > window.innerHeight - 96;

    setPopover({
      above,
      left,
      top: above ? rect.top - 6 : rect.bottom + 6,
    });
  };

  return (
    <td
      className="activity-popover-cell"
      tabIndex={0}
      aria-describedby={popover ? tooltipId : undefined}
      onMouseEnter={(event) => showPopover(event.currentTarget)}
      onMouseLeave={() => setPopover(null)}
      onFocus={(event) => showPopover(event.currentTarget)}
      onBlur={() => setPopover(null)}
    >
      <span className="activity-cell-value">{children ?? value}</span>
      {popover &&
        createPortal(
          <span
            className={`activity-cell-popover${popover.above ? " activity-cell-popover--above" : ""}`}
            id={tooltipId}
            role="tooltip"
            style={{ left: popover.left, top: popover.top }}
          >
            {value}
          </span>,
          document.body,
        )}
    </td>
  );
}

function StatusGauge({
  active,
  buttonLabel,
  onAction,
}: {
  active: boolean;
  buttonLabel: string;
  onAction: () => void;
}) {
  return (
    <div
      className={`status-gauge${active ? " status-gauge--active" : " status-gauge--stopped"}`}
    >
      <svg aria-hidden="true" viewBox="0 0 120 120">
        <circle className="gauge-track" cx="60" cy="60" r="42" />
        <circle className="gauge-progress" cx="60" cy="60" r="42" />
      </svg>
      <button type="button" className="gauge-start" onClick={onAction}>
        {buttonLabel}
      </button>
    </div>
  );
}

function displayPresetName(source: string) {
  if (!source) return "LOADING…";
  const filename = source.split(/[\\/]/).pop() ?? source;
  return filename === DEFAULT_KEYBINDINGS_FILENAME
    ? "DEFAULT"
    : `CUSTOM (${filename})`;
}

function displayEndpoint(_endpoint: string, hostAddress = "127.0.0.1") {
  return hostAddress;
}

function formatPairingTime(remainingSeconds: number) {
  const minutes = Math.floor(remainingSeconds / 60);
  const seconds = remainingSeconds % 60;
  return minutes + ":" + seconds.toString().padStart(2, "0");
}

export default function App() {
  const snapshot = useBridgeState();
  const [hostAddress, setHostAddress] = useState("127.0.0.1");

  useEffect(() => {
    void invoke<string>("get_host_address")
      .then((address) => {
        if (typeof address === "string" && address.length > 0) {
          setHostAddress(address);
        }
      })
      .catch((error) =>
        console.error("Unable to determine host address", error),
      );
  }, []);

  const serverLabel = SERVER_LABELS[snapshot.serverStatus];
  const [activeTab, setActiveTab] = useState<
    "Status" | "Activity" | "Connections" | "Mappings" | "Help" | "About"
  >("Status");
  const [pairingError, setPairingError] = useState<string | null>(null);
  const [pairingClock, setPairingClock] = useState(() => Date.now());
  const [keybindings, setKeybindings] = useState<KeybindingPreview | null>(
    null,
  );
  const [mappingError, setMappingError] = useState<string | null>(null);
  const [mappingBusy, setMappingBusy] = useState(false);

  useEffect(() => {
    if (!snapshot.activePairing?.expiresAt) return;

    const timer = window.setInterval(() => setPairingClock(Date.now()), 250);
    return () => window.clearInterval(timer);
  }, [snapshot.activePairing?.expiresAt]);

  const pairingSecondsRemaining = snapshot.activePairing
    ? Math.max(
        0,
        Math.ceil(
          (Date.parse(snapshot.activePairing.expiresAt) - pairingClock) / 1000,
        ),
      )
    : 0;

  const loadMappings = async () => {
    setMappingBusy(true);
    setMappingError(null);
    try {
      setKeybindings(
        await invoke<KeybindingPreview>("load_default_keybindings"),
      );
    } catch (error) {
      setMappingError(
        typeof error === "string" ? error : "Unable to load keybindings",
      );
    } finally {
      setMappingBusy(false);
    }
  };

  useEffect(() => {
    const loadTimer = window.setTimeout(() => void loadMappings(), 0);
    return () => window.clearTimeout(loadTimer);
  }, []);

  const isActive = snapshot.serverStatus === "active";
  const buttonLabel = isActive ? "STOP" : "START";
  const keyMappingState = mappingError
    ? "UNAVAILABLE"
    : keybindings
      ? displayPresetName(keybindings.source)
      : "LOADING…";
  const keyMappingTone =
    keyMappingState === "DEFAULT"
      ? "default"
      : keyMappingState.startsWith("CUSTOM ")
        ? "custom"
        : "neutral";

  const handleServerAction = async () => {
    try {
      await invoke(isActive ? "stop_server" : "start_server");
    } catch (error) {
      console.error("Unable to update bridge server", error);
    }
  };
  const actionHistory =
    snapshot.actionHistory.length > 0
      ? snapshot.actionHistory
      : snapshot.lastAction
        ? [snapshot.lastAction]
        : [];
  const activity = [...actionHistory]
    .sort((a, b) => b.sequence - a.sequence)
    .map(
      (attempt) =>
        [
          attempt.sequence,
          attempt.receivedAt.slice(11, 19),
          attempt.action,
          attempt.label ?? "Unknown action",
          attempt.shortcut?.display ?? "—",
          attempt.validation === "accepted" ? "OK" : "ERR",
        ] as const,
    );
  const visibleActivity =
    activeTab === "Status" ? activity.slice(0, 10) : activity;
  const connectedCount = snapshot.devices.filter(
    (device) => device.status === "connected",
  ).length;

  const handleAddDevice = async () => {
    setPairingError(null);
    try {
      if (snapshot.serverStatus !== "active") {
        await invoke("start_server");
      }
      await invoke("start_pairing");
    } catch (error) {
      setPairingError(
        typeof error === "string" ? error : "Unable to start pairing",
      );
      console.error("Unable to start device pairing", error);
    }
  };

  const handleCancelPairing = async () => {
    try {
      await invoke("cancel_pairing");
    } catch (error) {
      setPairingError(
        typeof error === "string" ? error : "Unable to cancel pairing",
      );
      console.error("Unable to cancel device pairing", error);
    }
  };

  return (
    <main className="app-window">
      <h1 className="sr-only">GGanbu Bridge</h1>
      <div className="app-body">
        <aside className="sidebar">
          <nav aria-label="Primary navigation">
            {NAV_ITEMS.map((item) => (
              <button
                type="button"
                className={`nav-item${item.label === activeTab ? " nav-item--active" : ""}`}
                aria-current={item.label === activeTab ? "page" : undefined}
                disabled={false}
                onClick={() => {
                  if (
                    item.label === "Status" ||
                    item.label === "Activity" ||
                    item.label === "Connections" ||
                    item.label === "Mappings" ||
                    item.label === "Help" ||
                    item.label === "About"
                  )
                    setActiveTab(item.label);
                }}
                key={item.label}
              >
                <Icon name={item.icon} />
                <span>{item.label}</span>
              </button>
            ))}
          </nav>
        </aside>

        <section
          className={`status-view status-view--${activeTab.toLowerCase()}`}
          aria-label={activeTab + " tab"}
        >
          {activeTab === "Help" ? (
            <section className="help-view" aria-labelledby="help-title">
              <header className="help-header">
                <div>
                  <p className="about-eyebrow">QUICK REFERENCE</p>
                  <h2 id="help-title">Keep the bridge connected.</h2>
                  <p className="about-lede">
                    A few checks cover most setup and connection issues.
                  </p>
                </div>
                <span className="help-status-mark" aria-hidden="true">
                  ?
                </span>
              </header>
              <div className="help-content">
                <div className="help-list">
                  <article className="help-item">
                    <span className="about-card-index">01</span>
                    <div>
                      <h3>Start the bridge</h3>
                      <p>
                        Open Status and press START. The bridge must be active
                        before a device can pair.
                      </p>
                    </div>
                  </article>
                  <article className="help-item">
                    <span className="about-card-index">02</span>
                    <div>
                      <h3>Connect GGanbu.app</h3>
                      <p>
                        Keep your device and desktop on the same network, then
                        pair from GGanbu.app using the address shown in
                        Connections.
                      </p>
                    </div>
                  </article>
                  <article className="help-item">
                    <span className="about-card-index">03</span>
                    <div>
                      <h3>When actions do not fire</h3>
                      <p>
                        Check that the bridge is Active, the device is
                        connected, and your key mappings are loaded for the
                        correct game.
                      </p>
                    </div>
                  </article>
                  <article className="help-item">
                    <span className="about-card-index">04</span>
                    <div>
                      <h3>About Mappings</h3>
                      <p>
                        Mappings translate approved GGanbu actions into your
                        game’s keyboard shortcuts. Open Mappings, select the
                        correct .blkx file, and confirm the loaded bindings
                        before playing.
                      </p>
                    </div>
                  </article>
                </div>
                <div className="help-note">
                  <strong>Still stuck?</strong>
                  <p>
                    Restart the bridge, pair the device again, and review
                    Activity for the first failed action.
                  </p>
                </div>
              </div>
            </section>
          ) : activeTab === "About" ? (
            <section className="about-view" aria-labelledby="about-title">
              <header className="about-hero">
                <img src={appImage} alt="" className="about-logo" />
                <div>
                  <p className="about-eyebrow">GGANBU BRIDGE</p>
                  <h2 id="about-title">
                    A quiet link between GGanbu and your desktop.
                  </h2>
                  <p className="about-lede">
                    GGanbu Bridge connects GGanbu.app to your PC, translating
                    approved in-game actions into local keyboard shortcuts.
                  </p>
                  <p className="about-lede">
                    Everything happens locally! No remote command server, no
                    unnecessary relay.
                  </p>
                </div>
              </header>
              <div className="about-content">
                <div className="about-grid">
                  <article className="about-card">
                    <span className="about-card-index">01</span>
                    <h3>Local by design</h3>
                    <p>
                      GGanbu.app connects directly to this bridge on the same
                      network.
                    </p>
                  </article>
                  <article className="about-card">
                    <span className="about-card-index">02</span>
                    <h3>Focused on control</h3>
                    <p>
                      Only approved actions are resolved and forwarded as
                      shortcuts.
                    </p>
                  </article>
                  <article className="about-card">
                    <span className="about-card-index">03</span>
                    <h3>Know what the Bridge is doing</h3>
                    <p>
                      See connections, bridge health, and every received action
                      in one place.
                    </p>
                  </article>
                </div>
                <div className="about-note">
                  <span className="panel-title-mark" aria-hidden="true" />
                  <div>
                    <strong>Built to stay out of the way.</strong>
                    <p>
                      GGanbu Bridge has one job: connect GGanbu.app to your
                      game. Start it when you play, keep it in the background,
                      and close it when you’re done.
                    </p>
                  </div>
                </div>
                <div className="about-legal" aria-label="License information">
                  <p>Copyright © 2026 Game Buddy</p>
                  <p>
                    Licensed under the GNU Affero General Public License version
                    3 only. This program comes with no warranty.
                  </p>
                  <p>
                    War Thunder keybinding compatibility files are third-party
                    data credited to{" "}
                    <a
                      href="https://github.com/gszabi99/War-Thunder-Datamine"
                      target="_blank"
                      rel="noreferrer"
                    >
                      War-Thunder-Datamine
                    </a>
                    . Game Buddy is unofficial and is not affiliated with or
                    endorsed by Gaijin Entertainment.
                  </p>
                  <p>
                    <a href={LICENSE_URL} target="_blank" rel="noreferrer">
                      Read the license
                    </a>
                    <span aria-hidden="true"> · </span>
                    <a href={SOURCE_URL} target="_blank" rel="noreferrer">
                      Get the corresponding source
                    </a>
                  </p>
                </div>
              </div>
            </section>
          ) : activeTab === "Mappings" ? (
            <section className="mappings-view" aria-labelledby="mappings-title">
              <header className="mappings-toolbar">
                <div>
                  <h2 id="mappings-title">KEY MAPPINGS</h2>
                  <p>Load a War Thunder Key Mappings.</p>
                </div>
                <div className="mappings-toolbar-actions">
                  <button
                    type="button"
                    className="reset-mapping-button"
                    disabled={mappingBusy}
                    onClick={() => void loadMappings()}
                  >
                    Reset to default
                  </button>
                  <button
                    type="button"
                    className="add-device-button"
                    disabled={mappingBusy}
                    onClick={async () => {
                      setMappingBusy(true);
                      setMappingError(null);
                      try {
                        const selected = await invoke<KeybindingPreview | null>(
                          "pick_keybindings_file",
                        );
                        if (selected) setKeybindings(selected);
                      } catch (error) {
                        setMappingError(
                          error instanceof Error
                            ? error.message
                            : String(error),
                        );
                      } finally {
                        setMappingBusy(false);
                      }
                    }}
                  >
                    {mappingBusy ? "Loading…" : "Upload your Controls"}
                  </button>
                </div>
              </header>
              {mappingError && (
                <p className="mapping-error" role="alert">
                  {mappingError}
                </p>
              )}
              {keybindings && (
                <div className="mappings-content">
                  <div className="mapping-summary">
                    <div>
                      <span className="summary-label">ACTIVE PRESET</span>
                      <strong>{displayPresetName(keybindings.source)}</strong>
                    </div>
                    <div>
                      <span className="summary-label">RESOLVED</span>
                      <strong>{keybindings.bindings} mappings</strong>
                    </div>
                  </div>
                  <div className="mapping-table-wrap">
                    <table className="mapping-table">
                      <thead>
                        <tr>
                          <th>ACTION</th>
                          <th>KEY MAPPING</th>
                        </tr>
                      </thead>
                      <tbody>
                        {keybindings.hotkeys.map((row) => (
                          <tr key={row.action}>
                            <td>{row.action}</td>
                            <td>
                              <code>{row.binding}</code>
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </div>
              )}
            </section>
          ) : activeTab === "Connections" ? (
            <section
              className="connections-view"
              aria-labelledby="connections-title"
            >
              <header className="connections-toolbar">
                <div>
                  <h2 id="connections-title">CONNECTED DEVICES</h2>
                  <p>GGanbu.app can send actions through this bridge.</p>
                </div>
                <div className="connections-toolbar-actions">
                  <span className="connection-count">
                    <span className="row-dot" aria-hidden="true" />
                    {connectedCount} ACTIVE
                  </span>
                  <button
                    type="button"
                    className="add-device-button"
                    aria-label="Add device"
                    onClick={() => void handleAddDevice()}
                    disabled={snapshot.activePairing !== null}
                  >
                    <span aria-hidden="true">+</span>{" "}
                    {snapshot.activePairing ? "Pairing…" : "Add device"}
                  </button>
                </div>
              </header>
              <div className="connections-content">
                {snapshot.activePairing && (
                  <section
                    className="pairing-card"
                    aria-live="polite"
                    aria-label="Device pairing"
                  >
                    <div>
                      <strong>PAIRING CODE</strong>
                      <p>Enter this code on the GGanbu.app to pair.</p>
                    </div>
                    <code>{snapshot.activePairing.code}</code>
                    <span
                      className={
                        "pairing-timer" +
                        (pairingSecondsRemaining === 0
                          ? " pairing-timer--urgent"
                          : "")
                      }
                      role="timer"
                      aria-label={
                        "Pairing code expires in " +
                        formatPairingTime(pairingSecondsRemaining)
                      }
                    >
                      <svg
                        className="pairing-timer-ring"
                        viewBox="0 0 58 58"
                        aria-hidden="true"
                      >
                        <circle
                          className="pairing-timer-track"
                          cx="29"
                          cy="29"
                          r="24"
                        />
                        <circle
                          className="pairing-timer-progress"
                          cx="29"
                          cy="29"
                          r="24"
                          style={{
                            strokeDashoffset:
                              2 *
                              Math.PI *
                              24 *
                              (1 - pairingSecondsRemaining / 60),
                          }}
                        />
                      </svg>{" "}
                      <strong>
                        {formatPairingTime(pairingSecondsRemaining)}
                      </strong>
                    </span>
                    <button
                      type="button"
                      className="disconnect-button"
                      onClick={() => void handleCancelPairing()}
                    >
                      Cancel
                    </button>
                  </section>
                )}
                {pairingError && (
                  <p className="pairing-error" role="alert">
                    {pairingError}
                  </p>
                )}
                <div className="connection-summary">
                  <span className="summary-label">BRIDGE ADDRESS</span>
                  <strong>
                    {displayEndpoint(snapshot.endpoint, hostAddress)}
                  </strong>
                  <span>
                    Share this address with GGanbu.app on your local network.
                  </span>
                </div>
                <div className="device-list">
                  {snapshot.devices.map((device) => (
                    <article
                      className={`device-row device-row--${device.status}`}
                      key={device.id}
                    >
                      <div className="device-icon" aria-hidden="true">
                        <Icon name="connections" />
                      </div>
                      <div className="device-identity">
                        <strong>{device.name}</strong>
                        <span>Browser device</span>
                      </div>
                      <div className="device-network">
                        <strong>{hostAddress}:53177</strong>
                        <span>
                          Last seen {new Date(device.lastSeen).toLocaleString()}
                        </span>
                      </div>
                      <span
                        className={`device-status device-status--${device.status}`}
                      >
                        <span className="status-mark" aria-hidden="true" />
                        {device.status === "connected" ? "Connected" : "Idle"}
                      </span>
                      <button
                        type="button"
                        className="disconnect-button"
                        onClick={async () => {
                          try {
                            await invoke("remove_device", {
                              deviceId: device.id,
                            });
                          } catch (error) {
                            setPairingError(
                              error instanceof Error
                                ? error.message
                                : String(error),
                            );
                          }
                        }}
                      >
                        Remove
                      </button>
                    </article>
                  ))}
                  {snapshot.devices.length === 0 && (
                    <div className="connections-empty">
                      <strong>No devices connected</strong>
                      <span>Connected devices will appear here.</span>
                    </div>
                  )}
                </div>
              </div>
            </section>
          ) : (
            <div className="status-columns">
              <section
                className="activity-panel"
                aria-labelledby="activity-title"
                aria-live="polite"
              >
                <header className="activity-toolbar">
                  <h2 id="activity-title">ACTIVITY LOG</h2>
                </header>
                <div className="activity-table-wrap">
                  <table className="activity-table">
                    <thead>
                      <tr>
                        <th>TIME</th>
                        <th>ACTION ID</th>
                        <th>ACTION NAME</th>
                        <th>KEY</th>
                        <th>STATUS</th>
                      </tr>
                    </thead>
                    <tbody>
                      {visibleActivity.map(
                        ([sequence, time, action, name, key, status]) => (
                          <tr key={sequence}>
                            <td>
                              <span className="row-dot" aria-hidden="true" />
                              {time}
                            </td>
                            <ActivityCell value={action} />
                            <ActivityCell value={name} />
                            <ActivityCell value={key} />
                            <td>
                              <span
                                className={`result result--${status === "OK" ? "ok" : "error"}`}
                              >
                                {status}
                              </span>
                            </td>
                          </tr>
                        ),
                      )}
                      {visibleActivity.length === 0 && (
                        <tr>
                          <td colSpan={5} className="activity-empty">
                            No requests recorded yet.
                          </td>
                        </tr>
                      )}
                    </tbody>
                  </table>
                </div>
              </section>

              {activeTab === "Status" && (
                <aside
                  className="health-panel"
                  aria-labelledby="bridge-status-title"
                  aria-live="polite"
                >
                  <h2 id="bridge-status-title">
                    <span className="panel-title-mark" aria-hidden="true" />
                    BRIDGE STATUS
                  </h2>
                  <div className="health-summary">
                    <StatusGauge
                      active={isActive}
                      buttonLabel={buttonLabel}
                      onAction={() => void handleServerAction()}
                    />
                    <strong>{serverLabel}</strong>
                    <p>
                      {isActive
                        ? "The bridge is running normally."
                        : "The bridge is not running."}
                    </p>
                  </div>
                  <dl className="health-metrics">
                    <div>
                      <dt>ADDRESS</dt>
                      <dd>{displayEndpoint(snapshot.endpoint, hostAddress)}</dd>
                    </div>
                  </dl>
                  {snapshot.serverStatus === "error" && (
                    <section className="error-banner" role="alert">
                      <p className="error-title">Bridge unavailable</p>
                      <p>
                        {snapshot.serverError ??
                          "An unknown startup error occurred."}
                      </p>
                      <p className="error-help">
                        Check <code>GGANBU_BRIDGE_ALLOWED_ORIGINS</code> and
                        port 53177.
                      </p>
                    </section>
                  )}
                </aside>
              )}
            </div>
          )}
        </section>
      </div>

      <footer className="app-footer">
        <span>
          Key Mapping:{" "}
          <strong
            className={`footer-key-mapping footer-key-mapping--${keyMappingTone}`}
          >
            {keyMappingState}
          </strong>
        </span>
        <span>Version {APP_VERSION}</span>
      </footer>

      <div
        className="sr-only compatibility-region"
        aria-live="polite"
        aria-atomic="true"
      >
        <span>{isActive ? serverLabel : "Status " + serverLabel}</span>
        <span>Running</span>
        <span>
          {snapshot.lastMessage
            ? snapshot.lastMessage.text
            : "No message received yet"}
        </span>
        {snapshot.lastMessage && (
          <>
            <span>Sequence {snapshot.lastMessage.sequence}</span>
            <span>{snapshot.lastMessage.receivedAt}</span>
          </>
        )}
        <span>
          {snapshot.lastAction
            ? `${snapshot.lastAction.validation === "accepted" ? "Accepted" : "Rejected"} ${snapshot.lastAction.shortcut?.display ?? "Not resolved"} Request ${snapshot.lastAction.requestId} ${snapshot.lastAction.executed ? "Keyboard shortcut sent" : "No key was sent"}`
            : "No action received yet"}
        </span>
        {snapshot.lastAction && (
          <>
            <span>
              {snapshot.lastAction.validation === "accepted"
                ? "Accepted"
                : "Rejected"}
            </span>
            {!snapshot.lastAction.shortcut && <span>Not resolved</span>}
            <span>Request {snapshot.lastAction.requestId}</span>
            <span>{snapshot.lastAction.validationMessage}</span>
          </>
        )}
        {snapshot.allowedOrigins.map((origin) => (
          <span key={origin}>{origin}</span>
        ))}
        <span>{snapshot.endpoint}</span>
      </div>
    </main>
  );
}
