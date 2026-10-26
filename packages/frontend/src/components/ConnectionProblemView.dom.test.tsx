// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ConnectionProblem } from "../services/connectionProblem";
import { ConnectionProblemView, problemCopy } from "./ConnectionProblemView";

const oldPc = { name: "OLD-PC" };
const outdated: ConnectionProblem = {
  kind: "incompatible",
  detail: "Update OpenAIDE on your computer",
  clientProtocol: "2.3",
  server: { protocol: "2.2", appVersion: "0.40.0" },
  update: "computer",
};

describe("ConnectionProblemView", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
  });

  afterEach(() => {
    act(() => root.unmount());
    container.remove();
  });

  it("names the computer and the cause, and keeps the raw error behind Details", () => {
    const message = "App Server WebSocket link ended: open_timeout";
    act(() => root.render(<ConnectionProblemView message={message} place={oldPc} problem={{ kind: "unreachable", detail: message }} />));
    expect(container.querySelector("h2")?.textContent).toBe("Can’t reach OLD-PC");
    expect(container.querySelector("p")?.textContent).toBe("Check that it’s on and OpenAIDE is running there.");
    expect(container.querySelector(".connection-link-broken")?.textContent).toBe("This computerOLD-PC");
    expect(container.querySelector("details dd")?.textContent).toBe(message);
    expect(container.querySelector("h2, p")?.textContent).not.toContain("open_timeout");
  });

  it("marks the side to update with its app version and keeps protocols behind Details", () => {
    act(() => root.render(<ConnectionProblemView device="phone" message="Update OpenAIDE on your computer" place={oldPc} problem={outdated} />));
    expect(container.querySelector("h2")?.textContent).toBe("OLD-PC needs an update");
    expect(container.querySelector("p")?.textContent).toBe("Update OpenAIDE on OLD-PC, then try again.");
    expect(container.querySelector(".connection-link-mismatch")?.textContent).toBe("This phone≠OLD-PC0.40.0");
    expect(container.querySelector(".connection-end-note.stale")?.textContent).toBe("0.40.0");
    expect(container.querySelector("dl")?.textContent).toContain("Protocol 2.3");
    expect(container.querySelector("dl")?.textContent).toContain("OpenAIDE 0.40.0 · protocol 2.2");
  });

  it("offers trying again and the other places right on the page", () => {
    const retry = vi.fn();
    const use = vi.fn();
    const places = [{ id: "", label: "This phone" }, { id: "linux-key", label: "old-linux" }];
    act(() => root.render(<ConnectionProblemView message="x" onRetry={retry} onUse={use} place={oldPc} places={places} problem={{ kind: "unreachable", detail: "x" }} />));
    const buttons = [...container.querySelectorAll("button")];
    expect(buttons.map((button) => button.textContent)).toEqual(["Try again", "This phone", "old-linux"]);
    act(() => buttons[0]!.click());
    act(() => buttons[2]!.click());
    expect(retry).toHaveBeenCalledOnce();
    expect(use).toHaveBeenCalledWith("linux-key");
    act(() => root.render(<ConnectionProblemView message="x" place={oldPc} />));
    expect(container.querySelectorAll("button")).toHaveLength(0);
  });
});

describe("problemCopy", () => {
  it("covers this device, a newer computer, unknown versions, and an unnamed App Server", () => {
    expect(problemCopy({ kind: "unreachable", detail: "" }, { name: "This phone", self: "phone" }).title).toBe("Can’t reach OpenAIDE on this phone");
    expect(problemCopy({ ...outdated, update: "app" }, oldPc)).toEqual({
      title: "This app needs an update",
      line: "OLD-PC runs a newer OpenAIDE. Update this app, then try again.",
    });
    const unknown: ConnectionProblem = { kind: "incompatible", detail: "", clientProtocol: "2.3", update: "unknown" };
    expect(problemCopy(unknown, oldPc).title).toBe("OLD-PC runs a different version");
    expect(problemCopy({ kind: "failed", detail: "" }, undefined).title).toBe("Can’t connect to the App Server");
  });
});
