import { afterEach, describe, expect, it, vi } from "vitest";

import { uploadFile } from "./fileUpload";

type PlannedResponse = {
  kind: "error";
} | {
  kind: "load";
  status: number;
  body?: string;
  contentType?: string;
};

class FakeXMLHttpRequest {
  static planned: PlannedResponse[] = [];
  static requests: FakeXMLHttpRequest[] = [];

  readonly headers = new Map<string, string>();
  readonly listeners = new Map<string, Array<() => void>>();
  readonly uploadListeners = new Map<string, Array<(event: ProgressEvent) => void>>();
  readonly upload = {
    addEventListener: (type: string, listener: (event: ProgressEvent) => void) => {
      const listeners = this.uploadListeners.get(type) ?? [];
      listeners.push(listener);
      this.uploadListeners.set(type, listeners);
    },
  };
  method = "";
  url = "";
  status = 0;
  responseText = "";
  responseContentType: string | null = null;
  sentBody?: Blob;

  constructor() {
    FakeXMLHttpRequest.requests.push(this);
  }

  open(method: string, url: string) {
    this.method = method;
    this.url = url;
  }

  setRequestHeader(name: string, value: string) {
    this.headers.set(name, value);
  }

  getResponseHeader(name: string) {
    return name.toLowerCase() === "content-type" ? this.responseContentType : null;
  }

  addEventListener(type: string, listener: () => void) {
    const listeners = this.listeners.get(type) ?? [];
    listeners.push(listener);
    this.listeners.set(type, listeners);
  }

  send(body?: Blob) {
    this.sentBody = body;
    const response = FakeXMLHttpRequest.planned.shift();
    if (!response) throw new Error("Missing planned XMLHttpRequest response");
    if (body) {
      for (const listener of this.uploadListeners.get("progress") ?? []) {
        listener({ loaded: body.size, total: body.size, lengthComputable: true } as ProgressEvent);
      }
    }
    if (response.kind === "error") {
      for (const listener of this.listeners.get("error") ?? []) listener();
      return;
    }
    this.status = response.status;
    this.responseText = response.body ?? "";
    this.responseContentType = response.contentType ?? null;
    for (const listener of this.listeners.get("load") ?? []) listener();
  }

  abort() {
    for (const listener of this.listeners.get("abort") ?? []) listener();
  }
}

describe("web file upload transport", () => {
  afterEach(() => {
    FakeXMLHttpRequest.planned = [];
    FakeXMLHttpRequest.requests = [];
    vi.unstubAllGlobals();
  });

  it("falls back from one failed request to sequential chunks no larger than 512 KiB", async () => {
    vi.stubGlobal("XMLHttpRequest", FakeXMLHttpRequest);
    FakeXMLHttpRequest.planned = [
      { kind: "error" },
      { kind: "load", status: 202, body: '{"received":524288}' },
      { kind: "load", status: 202, body: '{"received":1048576}' },
      { kind: "load", status: 200, body: '{"attachment":{"handleId":"attachment-1"}}' },
    ];
    const file = new File([new Uint8Array(1_200_000)], "large.bin");
    const onProgress = vi.fn();

    const attachment = await uploadFile(
      "task-1",
      file,
      "client-1",
      onProgress,
      new AbortController().signal,
    );

    expect(attachment).toEqual({ handleId: "attachment-1" });
    expect(FakeXMLHttpRequest.requests.map((request) => request.url)).toEqual([
      "/__openaide-app-server/upload",
      "/__openaide-app-server/upload/chunk",
      "/__openaide-app-server/upload/chunk",
      "/__openaide-app-server/upload/chunk",
    ]);
    const chunks = FakeXMLHttpRequest.requests.slice(1);
    expect(chunks.map((request) => request.sentBody?.size)).toEqual([524_288, 524_288, 151_424]);
    expect(chunks.map((request) => request.headers.get("X-OpenAIDE-Upload-Offset")))
      .toEqual(["0", "524288", "1048576"]);
    expect(new Set(chunks.map((request) => request.headers.get("X-OpenAIDE-Upload-Id"))).size).toBe(1);
    expect(chunks.every((request) => request.headers.get("X-OpenAIDE-Upload-Size") === "1200000")).toBe(true);
    expect(onProgress).toHaveBeenLastCalledWith({ loaded: 1_200_000, total: 1_200_000 });
  });

  it("falls back when an intermediary rejects the single request with an HTML 403", async () => {
    vi.stubGlobal("XMLHttpRequest", FakeXMLHttpRequest);
    FakeXMLHttpRequest.planned = [
      { kind: "load", status: 403, contentType: "text/html; charset=utf-8", body: "<html>Denied</html>" },
      { kind: "load", status: 202, body: '{"received":524288}' },
      { kind: "load", status: 202, body: '{"received":1048576}' },
      { kind: "load", status: 200, body: '{"attachment":{"handleId":"attachment-403"}}' },
    ];

    const attachment = await uploadFile(
      "task-1",
      new File([new Uint8Array(1_300_000)], "work-network.bin"),
      "client-1",
      vi.fn(),
      new AbortController().signal,
    );

    expect(attachment).toEqual({ handleId: "attachment-403" });
    expect(FakeXMLHttpRequest.requests.map((request) => request.url)).toEqual([
      "/__openaide-app-server/upload",
      "/__openaide-app-server/upload/chunk",
      "/__openaide-app-server/upload/chunk",
      "/__openaide-app-server/upload/chunk",
    ]);
  });

  it("preserves image metadata across the single request and every fallback chunk", async () => {
    vi.stubGlobal("XMLHttpRequest", FakeXMLHttpRequest);
    FakeXMLHttpRequest.planned = [
      { kind: "load", status: 403, contentType: "text/html", body: "<html>Denied</html>" },
      { kind: "load", status: 202, body: '{"received":524288}' },
      { kind: "load", status: 202, body: '{"received":1048576}' },
      { kind: "load", status: 200, body: '{"attachment":{"handleId":"image-1","label":"scan.png"}}' },
    ];
    const file = new File([new Uint8Array(1_300_000)], "scan.png", { type: "image/png" });

    await uploadFile(
      "task-1",
      file,
      "client-1",
      vi.fn(),
      new AbortController().signal,
      undefined,
      { kind: "image", mimeType: "image/png" },
    );

    expect(FakeXMLHttpRequest.requests).toHaveLength(4);
    expect(FakeXMLHttpRequest.requests.every((request) =>
      request.headers.get("X-OpenAIDE-Attachment-Kind") === "image"
      && request.headers.get("X-OpenAIDE-Mime-Type") === "image/png"
    )).toBe(true);
  });

  it("does not hide an App Server validation error behind chunk fallback", async () => {
    vi.stubGlobal("XMLHttpRequest", FakeXMLHttpRequest);
    FakeXMLHttpRequest.planned = [
      { kind: "load", status: 400, body: '{"error":{"message":"Attachment is not allowed."}}' },
    ];

    await expect(uploadFile(
      "task-1",
      new File(["data"], "blocked.bin"),
      "client-1",
      vi.fn(),
      new AbortController().signal,
    )).rejects.toThrow("Attachment is not allowed.");

    expect(FakeXMLHttpRequest.requests.map((request) => request.url)).toEqual([
      "/__openaide-app-server/upload",
    ]);
  });

  it("does not retry an App Server JSON 403 through the chunk endpoint", async () => {
    vi.stubGlobal("XMLHttpRequest", FakeXMLHttpRequest);
    FakeXMLHttpRequest.planned = [{
      kind: "load",
      status: 403,
      contentType: "application/json",
      body: '{"error":{"message":"Client authentication failed."}}',
    }];

    await expect(uploadFile(
      "task-1",
      new File(["data"], "unauthorized.bin"),
      "client-1",
      vi.fn(),
      new AbortController().signal,
    )).rejects.toThrow("Client authentication failed.");

    expect(FakeXMLHttpRequest.requests.map((request) => request.url)).toEqual([
      "/__openaide-app-server/upload",
    ]);
  });

  it("retries directly in chunk mode after the fallback itself fails", async () => {
    vi.stubGlobal("XMLHttpRequest", FakeXMLHttpRequest);
    FakeXMLHttpRequest.planned = [
      { kind: "error" },
      { kind: "load", status: 500, body: '{"error":{"message":"Temporary chunk failure."}}' },
      { kind: "load", status: 204 },
      { kind: "load", status: 200, body: '{"attachment":{"handleId":"attachment-2"}}' },
    ];
    const file = new File(["retry"], "retry.bin");

    await expect(uploadFile(
      "task-1",
      file,
      "client-1",
      vi.fn(),
      new AbortController().signal,
    )).rejects.toThrow("Temporary chunk failure.");
    const attachment = await uploadFile(
      "task-1",
      file,
      "client-1",
      vi.fn(),
      new AbortController().signal,
    );

    expect(attachment).toEqual({ handleId: "attachment-2" });
    expect(FakeXMLHttpRequest.requests.map((request) => request.url)).toEqual([
      "/__openaide-app-server/upload",
      "/__openaide-app-server/upload/chunk",
      "/__openaide-app-server/upload/chunk",
      "/__openaide-app-server/upload/chunk",
    ]);
    expect(FakeXMLHttpRequest.requests[2].headers.get("X-OpenAIDE-Upload-Cancel")).toBe("true");
  });
});
