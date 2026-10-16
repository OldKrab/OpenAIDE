import { readdirSync, readFileSync } from "node:fs";
import { afterEach, expect, it, vi } from "vitest";
import type { LocalHttpConnection } from "./desktopBootstrap";
import { createDesktopFileUploads, refuseUnclaimedFileDrops } from "./desktopFileUploads";

class FakeXMLHttpRequest {
  static requests: FakeXMLHttpRequest[] = [];
  readonly headers = new Map<string, string>();
  readonly listeners = new Map<string, () => void>();
  readonly upload = { addEventListener: () => undefined };
  url = "";
  status = 0;
  timeout = 0;
  responseText = "";

  constructor() {
    FakeXMLHttpRequest.requests.push(this);
  }

  open(_method: string, url: string) {
    this.url = url;
  }

  setRequestHeader(name: string, value: string) {
    this.headers.set(name, value);
  }

  getResponseHeader() {
    return "application/json";
  }

  addEventListener(type: string, listener: () => void) {
    this.listeners.set(type, listener);
  }

  send() {
    this.status = 200;
    this.responseText = `{"attachment":{"handleId":"handle-${FakeXMLHttpRequest.requests.length}"}}`;
    this.listeners.get("load")?.();
  }
}

afterEach(() => {
  FakeXMLHttpRequest.requests = [];
  vi.unstubAllGlobals();
});

const connectionTo = (endpointUrl: string, authToken = "token-1"): LocalHttpConnection =>
  ({ kind: "localHttp", endpointUrl, authToken });

function uploadThrough(connection: () => LocalHttpConnection, log = vi.fn()) {
  const files = createDesktopFileUploads(connection, "client-1", log);
  if (files.kind !== "webUpload") throw new Error("Desktop must upload attached files.");
  return files.upload("task-1", new File(["data"], "notes.pdf"), vi.fn(), new AbortController().signal);
}

it("uploads to the App Server in use now with its bearer token", async () => {
  vi.stubGlobal("XMLHttpRequest", FakeXMLHttpRequest);
  let connection = connectionTo("http://127.0.0.1:4100/probe");

  await expect(uploadThrough(() => connection)).resolves.toEqual({ handleId: "handle-1" });
  connection = connectionTo("http://127.0.0.1:4300/probe", "token-2");
  await expect(uploadThrough(() => connection)).resolves.toEqual({ handleId: "handle-2" });

  expect(FakeXMLHttpRequest.requests.map((request) => [request.url, request.headers.get("Authorization")])).toEqual([
    ["http://127.0.0.1:4100/upload", "Bearer token-1"],
    ["http://127.0.0.1:4300/upload", "Bearer token-2"],
  ]);
  expect(FakeXMLHttpRequest.requests[0].headers.get("X-OpenAIDE-Client-Instance-Id")).toBe("client-1");
  expect(FakeXMLHttpRequest.requests[0].headers.get("X-OpenAIDE-Task-Id")).toBe("task-1");
});

it("keeps a paired computer's gateway base path", async () => {
  vi.stubGlobal("XMLHttpRequest", FakeXMLHttpRequest);

  await uploadThrough(() => connectionTo("http://127.0.0.1:4200/base/probe/"));

  expect(FakeXMLHttpRequest.requests[0].url).toBe("http://127.0.0.1:4200/base/upload");
});

it("fails an endpoint it cannot address without a request", async () => {
  vi.stubGlobal("XMLHttpRequest", FakeXMLHttpRequest);
  const log = vi.fn();

  await expect(uploadThrough(() => connectionTo("http://127.0.0.1:4100/elsewhere"), log))
    .rejects.toThrow("File upload is unavailable.");

  expect(FakeXMLHttpRequest.requests).toEqual([]);
  expect(log).toHaveBeenCalledWith("warn", "file_upload_completed", expect.objectContaining({ error_kind: "endpoint" }));
});

function dragEvent(type: "dragover" | "drop", types: string[], claimed = false) {
  const event = new Event(type, { cancelable: true });
  Object.defineProperty(event, "dataTransfer", { value: { types, dropEffect: "copy" } });
  if (claimed) event.preventDefault();
  return event as DragEvent;
}

it("refuses only file drags that the Task surface left unclaimed", () => {
  const window = new EventTarget();
  refuseUnclaimedFileDrops(window as unknown as Window);
  const unclaimed = [dragEvent("dragover", ["Files"]), dragEvent("drop", ["Files"])];
  const claimed = dragEvent("dragover", ["Files"], true);
  const text = dragEvent("drop", ["text/plain"]);

  for (const event of [...unclaimed, claimed, text]) window.dispatchEvent(event);

  expect(unclaimed.map((event) => event.defaultPrevented)).toEqual([true, true]);
  expect(unclaimed[0].dataTransfer?.dropEffect).toBe("none");
  expect(claimed.dataTransfer?.dropEffect).toBe("copy");
  expect(text.defaultPrevented).toBe(false);
});

it("hands dropped files to the webview in every Desktop window configuration", () => {
  const configDirectory = new URL("../src-tauri/", import.meta.url);
  const windows = readdirSync(configDirectory)
    .filter((name) => /^tauri(\..+)?\.conf\.json$/.test(name))
    .flatMap((name) => {
      const config = JSON.parse(readFileSync(new URL(name, configDirectory), "utf8")) as {
        app?: { windows?: Array<{ dragDropEnabled?: boolean }> };
      };
      return (config.app?.windows ?? []).map((window) => [name, window.dragDropEnabled]);
    });

  expect(windows.length).toBeGreaterThan(0);
  expect(windows.filter(([, enabled]) => enabled !== false)).toEqual([]);
});
