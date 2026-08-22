// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import type { Event, EventCallback } from "@tauri-apps/api/event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import App from "./App";
import type { BridgeSnapshot } from "./bridge/types";
import { APP_VERSION } from "./version";

const tauri = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  unlisten: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: tauri.listen }));

const baseSnapshot: BridgeSnapshot = {
  revision: 1,
  applicationStatus: "running",
  serverStatus: "active",
  endpoint: "http://0.0.0.0:53177",
  serverError: null,
  allowedOrigins: ["http://localhost:5173", "https://app.example.com"],
  lastMessage: null,
  lastAction: null,
  actionHistory: [],
  activePairing: null,
  devices: [],
  activeConnectionCount: 0,
};

function repeatedAction(
  sequence: number,
): BridgeSnapshot["actionHistory"][number] {
  return {
    action: "ID_TACTICAL_MAP",
    requestId: `request-${sequence}`,
    label: "Tactical Map",
    platform: "linux",
    shortcut: {
      keyCodes: [20],
      modifiers: [],
      keys: ["T"],
      display: "T",
    },
    validation: "accepted",
    validationMessage: "Accepted",
    executed: false,
    receivedAt: "2026-08-07T23:52:45.000Z",
    sequence,
  };
}

function eventFor(snapshot: BridgeSnapshot): Event<BridgeSnapshot> {
  return {
    event: "bridge-state-changed",
    id: 1,
    payload: snapshot,
  };
}

describe("GGanbu Bridge status screen", () => {
  let eventCallback: EventCallback<BridgeSnapshot> | undefined;

  beforeEach(() => {
    tauri.invoke.mockReset();
    tauri.listen.mockReset();
    tauri.unlisten.mockReset();
    eventCallback = undefined;
    tauri.listen.mockImplementation(
      async (_event: string, callback: EventCallback<BridgeSnapshot>) => {
        eventCallback = callback;
        return tauri.unlisten;
      },
    );
    tauri.invoke.mockResolvedValue(baseSnapshot);
  });

  afterEach(cleanup);

  it("renders the initial starting and empty states accessibly", () => {
    tauri.listen.mockReturnValue(new Promise(() => undefined));
    render(<App />);

    expect(
      screen.getByRole("heading", { name: "GGanbu Bridge" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Not Running")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "START" })).toBeInTheDocument();
    expect(screen.getByText("No message received yet")).toBeInTheDocument();
    expect(screen.getByText("No action received yet")).toBeInTheDocument();
    expect(screen.getByText("Running")).toBeInTheDocument();
    expect(document.querySelectorAll('[aria-live="polite"]')).toHaveLength(3);
  });

  it("starts the server before pairing when Add device is clicked", async () => {
    tauri.invoke.mockResolvedValue({
      ...baseSnapshot,
      serverStatus: "stopped",
    });
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "Connections" }));
    fireEvent.click(screen.getByRole("button", { name: "Add device" }));

    await waitFor(() => {
      expect(tauri.invoke.mock.calls).toEqual(
        expect.arrayContaining([["start_server"], ["start_pairing"]]),
      );
    });
    expect(tauri.invoke.mock.invocationCallOrder.at(-1)).toBeGreaterThan(
      tauri.invoke.mock.invocationCallOrder[
        tauri.invoke.mock.calls.findIndex(([name]) => name === "start_server")
      ],
    );
  });

  it("renders a running server and currently loaded origins", async () => {
    render(<App />);

    expect(
      await screen.findByRole("button", { name: "STOP" }),
    ).toBeInTheDocument();
    expect(screen.getByText("http://0.0.0.0:53177")).toBeInTheDocument();
    expect(screen.getByText("https://app.example.com")).toBeInTheDocument();
    expect(screen.getByText("No message received yet")).toBeInTheDocument();
    expect(screen.getByText(`Version ${APP_VERSION}`)).toBeInTheDocument();
    expect(screen.queryByText("Ready")).not.toBeInTheDocument();
    expect(
      screen.queryByText("Connected to GameBuddy"),
    ).not.toBeInTheDocument();
  });

  it("shows the license and corresponding source in About", async () => {
    render(<App />);

    fireEvent.click(await screen.findByRole("button", { name: "About" }));

    expect(screen.getByText("Copyright © 2026 Game Buddy")).toBeInTheDocument();
    expect(
      screen.getByText(/GNU Affero General Public License version 3 only/),
    ).toBeInTheDocument();
    expect(screen.getByText(/no warranty/)).toBeInTheDocument();
    expect(
      screen.getByRole("link", { name: "Read the license" }),
    ).toHaveAttribute(
      "href",
      `https://github.com/Game-Buddy/GGanbu-App-Bridge/blob/v${APP_VERSION}/LICENSE`,
    );
    expect(
      screen.getByRole("link", { name: "Get the corresponding source" }),
    ).toHaveAttribute(
      "href",
      `https://github.com/Game-Buddy/GGanbu-App-Bridge/tree/v${APP_VERSION}`,
    );
  });

  it("renders accepted messages immediately from events", async () => {
    render(<App />);
    await waitFor(() => expect(eventCallback).toBeDefined());

    act(() => {
      eventCallback?.(
        eventFor({
          ...baseSnapshot,
          revision: 2,
          lastMessage: {
            text: "Hello\nfrom the browser",
            receivedAt: "2026-08-03T12:34:56.789Z",
            sequence: 1,
          },
        }),
      );
    });

    expect(screen.getByText(/Hello\s+from the browser/)).toBeInTheDocument();
    expect(screen.getByText("Sequence 1")).toBeInTheDocument();
    expect(screen.getByText(/2026-08-03T12:34:56.789Z/)).toBeInTheDocument();
  });

  it("renders accepted action resolutions without claiming execution", async () => {
    render(<App />);
    await waitFor(() => expect(eventCallback).toBeDefined());

    act(() => {
      eventCallback?.(
        eventFor({
          ...baseSnapshot,
          revision: 2,
          lastAction: {
            action: "ID_SCOUT_UAV",
            requestId: "request-001",
            label: "Scout UAV",
            platform: "linux",
            shortcut: {
              keyCodes: [56, 22],
              modifiers: ["alt"],
              keys: ["U"],
              display: "Alt + U",
            },
            validation: "accepted",
            validationMessage:
              "Preview resolved; keyboard execution was not requested.",
            executed: false,
            receivedAt: "2026-08-04T09:30:00.000Z",
            sequence: 1,
          },
        }),
      );
    });

    expect(screen.getByText("ID_SCOUT_UAV")).toBeInTheDocument();
    expect(screen.getByText("Alt + U")).toBeInTheDocument();
    expect(screen.getByText("Accepted")).toBeInTheDocument();
    expect(screen.getByText("Request request-001")).toBeInTheDocument();
    expect(screen.getByText(/No key was sent/)).toBeInTheDocument();
  });

  it("renders rejected unknown actions and no resolved shortcut", async () => {
    render(<App />);
    await waitFor(() => expect(eventCallback).toBeDefined());

    act(() => {
      eventCallback?.(
        eventFor({
          ...baseSnapshot,
          revision: 2,
          lastAction: {
            action: "fire_weapon",
            requestId: "request-unknown",
            label: null,
            platform: "linux",
            shortcut: null,
            validation: "rejected",
            validationMessage: "Rejected: action is not allowlisted.",
            executed: false,
            receivedAt: "2026-08-04T09:31:00.000Z",
            sequence: 1,
          },
        }),
      );
    });

    expect(screen.getByText("fire_weapon")).toBeInTheDocument();
    expect(screen.getByText("Rejected")).toBeInTheDocument();
    expect(screen.getByText("Not resolved")).toBeInTheDocument();
    expect(screen.getByText(/not allowlisted/)).toBeInTheDocument();
  });

  it("keeps the status view capped at ten repeated activity entries", async () => {
    render(<App />);
    await waitFor(() => expect(eventCallback).toBeDefined());

    act(() => {
      eventCallback?.(
        eventFor({
          ...baseSnapshot,
          revision: 2,
          actionHistory: Array.from({ length: 15 }, (_, index) =>
            repeatedAction(index + 1),
          ),
        }),
      );
    });

    expect(screen.getAllByRole("row")).toHaveLength(11);

    fireEvent.click(screen.getByRole("button", { name: "Activity" }));
    expect(screen.getAllByRole("row")).toHaveLength(16);
    expect(
      screen.queryByRole("heading", { name: "BRIDGE STATUS" }),
    ).not.toBeInTheDocument();

    const actionCell = screen
      .getAllByRole("cell", { name: "ID_TACTICAL_MAP" })[0]
      .closest("td");
    expect(actionCell).not.toBeNull();
    fireEvent.mouseEnter(actionCell!);
    expect(screen.getByRole("tooltip")).toHaveTextContent("ID_TACTICAL_MAP");
    fireEvent.mouseLeave(actionCell!);
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Status" }));
    expect(screen.getAllByRole("row")).toHaveLength(11);
    expect(
      screen.getByRole("heading", { name: "BRIDGE STATUS" }),
    ).toBeInTheDocument();
  });

  it("shows actionable server errors", async () => {
    tauri.invoke.mockResolvedValue({
      ...baseSnapshot,
      revision: 3,
      serverStatus: "error",
      serverError: "Unable to listen on 0.0.0.0:53177: address already in use",
    });
    render(<App />);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "address already in use",
    );
    expect(screen.getByRole("alert")).toHaveTextContent(
      "GGANBU_BRIDGE_ALLOWED_ORIGINS",
    );
    expect(screen.getByRole("alert")).toHaveTextContent("port 53177");
  });

  it("does not let a stale command snapshot overwrite a newer event", async () => {
    let resolveInvoke: (snapshot: BridgeSnapshot) => void = () => undefined;
    tauri.invoke.mockReturnValue(
      new Promise<BridgeSnapshot>((resolve) => {
        resolveInvoke = resolve;
      }),
    );
    render(<App />);
    await waitFor(() => expect(eventCallback).toBeDefined());

    act(() => {
      eventCallback?.(
        eventFor({
          ...baseSnapshot,
          revision: 7,
          lastMessage: {
            text: "newer event",
            receivedAt: "2026-08-03T12:34:56.789Z",
            sequence: 4,
          },
        }),
      );
    });
    await act(async () => {
      resolveInvoke({ ...baseSnapshot, revision: 6, lastMessage: null });
    });

    expect(screen.getByText("newer event")).toBeInTheDocument();
    expect(
      screen.queryByText("No message received yet"),
    ).not.toBeInTheDocument();
  });

  it("unregisters the Tauri event listener on unmount", async () => {
    const view = render(<App />);
    await waitFor(() => expect(tauri.listen).toHaveBeenCalledOnce());
    await waitFor(() =>
      expect(tauri.invoke).toHaveBeenCalledWith("get_bridge_snapshot"),
    );

    view.unmount();
    expect(tauri.unlisten).toHaveBeenCalledOnce();
  });
});
