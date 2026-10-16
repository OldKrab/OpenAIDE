import { afterEach, describe, expect, it, vi } from "vitest";

import { uploadFileOverHttp, type FileUploadRequest } from "./fileUploadHttp";

type PlannedResponse = {
  kind: "error";
} | {
  kind: "timeout";
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
  timeout = 0;
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
    if (response.kind === "timeout") {
      for (const listener of this.listeners.get("timeout") ?? []) listener();
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

function fixedChunkSizes(totalBytes: number, chunkBytes: number) {
  const sizes: number[] = [];
  for (let offset = 0; offset < totalBytes; offset += chunkBytes) {
    sizes.push(Math.min(chunkBytes, totalBytes - offset));
  }
  return sizes;
}

function upload(file: File, overrides: Partial<FileUploadRequest> = {}) {
  return uploadFileOverHttp({
    target: { url: "/__openaide-app-server/upload" },
    taskId: "task-1",
    file,
    clientInstanceId: "client-1",
    onProgress: vi.fn(),
    signal: new AbortController().signal,
    ...overrides,
  });
}

describe("file upload transport", () => {
  afterEach(() => {
    FakeXMLHttpRequest.planned = [];
    FakeXMLHttpRequest.requests = [];
    vi.unstubAllGlobals();
  });

  it("falls back to sequential chunks no larger than 64 KiB", async () => {
    vi.stubGlobal("XMLHttpRequest", FakeXMLHttpRequest);
    const expectedChunkSizes = fixedChunkSizes(1_200_000, 64 * 1024);
    FakeXMLHttpRequest.planned = [
      { kind: "error" },
      ...expectedChunkSizes.map((_, index) => index === expectedChunkSizes.length - 1
        ? { kind: "load" as const, status: 200, body: '{"attachment":{"handleId":"attachment-1"}}' }
        : { kind: "load" as const, status: 202, body: '{"received":true}' }),
    ];
    const file = new File([new Uint8Array(1_200_000)], "large.bin");
    const onProgress = vi.fn();

    const attachment = await upload(file, { onProgress });

    expect(attachment).toEqual({ handleId: "attachment-1" });
    expect(FakeXMLHttpRequest.requests.map((request) => request.url)).toEqual([
      "/__openaide-app-server/upload",
      ...expectedChunkSizes.map(() => "/__openaide-app-server/upload/chunk"),
    ]);
    const chunks = FakeXMLHttpRequest.requests.slice(1);
    expect(chunks.map((request) => request.sentBody?.size)).toEqual(expectedChunkSizes);
    expect(chunks.map((request) => request.headers.get("X-OpenAIDE-Upload-Offset")))
      .toEqual(expectedChunkSizes.map((_, index) => String(index * 64 * 1024)));
    expect(new Set(chunks.map((request) => request.headers.get("X-OpenAIDE-Upload-Id"))).size).toBe(1);
    expect(chunks.every((request) => request.headers.get("X-OpenAIDE-Upload-Size") === "1200000")).toBe(true);
    expect(onProgress).toHaveBeenLastCalledWith({ loaded: 1_200_000, total: 1_200_000 });
  });

  it("falls back when an intermediary rejects the single request with an HTML 403", async () => {
    vi.stubGlobal("XMLHttpRequest", FakeXMLHttpRequest);
    const expectedChunkSizes = fixedChunkSizes(1_300_000, 64 * 1024);
    FakeXMLHttpRequest.planned = [
      { kind: "load", status: 403, contentType: "text/html; charset=utf-8", body: "<html>Denied</html>" },
      ...expectedChunkSizes.map((_, index) => index === expectedChunkSizes.length - 1
        ? { kind: "load" as const, status: 200, body: '{"attachment":{"handleId":"attachment-403"}}' }
        : { kind: "load" as const, status: 202, body: '{"received":true}' }),
    ];

    const attachment = await upload(new File([new Uint8Array(1_300_000)], "work-network.bin"));

    expect(attachment).toEqual({ handleId: "attachment-403" });
    expect(FakeXMLHttpRequest.requests.map((request) => request.url)).toEqual([
      "/__openaide-app-server/upload",
      ...expectedChunkSizes.map(() => "/__openaide-app-server/upload/chunk"),
    ]);
  });

  it("fails a stalled fallback chunk with a retryable timeout", async () => {
    vi.stubGlobal("XMLHttpRequest", FakeXMLHttpRequest);
    FakeXMLHttpRequest.planned = [
      { kind: "error" },
      { kind: "timeout" },
      { kind: "load", status: 204 },
    ];

    await expect(upload(new File([new Uint8Array(600_000)], "stalled.bin"))).rejects.toThrow("File upload timed out. Retry the upload.");

    expect(FakeXMLHttpRequest.requests[1].timeout).toBe(300_000);
    expect(FakeXMLHttpRequest.requests[2].headers.get("X-OpenAIDE-Upload-Cancel")).toBe("true");
  });

  it("does not hide an App Server validation error behind chunk fallback", async () => {
    vi.stubGlobal("XMLHttpRequest", FakeXMLHttpRequest);
    FakeXMLHttpRequest.planned = [
      { kind: "load", status: 400, body: '{"error":{"message":"Attachment is not allowed."}}' },
    ];

    await expect(upload(new File(["data"], "blocked.bin"))).rejects.toThrow("Attachment is not allowed.");

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

    await expect(upload(new File(["data"], "unauthorized.bin"))).rejects.toThrow("Client authentication failed.");

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

    await expect(upload(file)).rejects.toThrow("Temporary chunk failure.");
    const attachment = await upload(file);

    expect(attachment).toEqual({ handleId: "attachment-2" });
    expect(FakeXMLHttpRequest.requests.map((request) => request.url)).toEqual([
      "/__openaide-app-server/upload",
      "/__openaide-app-server/upload/chunk",
      "/__openaide-app-server/upload/chunk",
      "/__openaide-app-server/upload/chunk",
    ]);
    expect(FakeXMLHttpRequest.requests[2].headers.get("X-OpenAIDE-Upload-Cancel")).toBe("true");
  });

  it("sends the shell's credentials with the upload, its chunks, and their cancellation", async () => {
    vi.stubGlobal("XMLHttpRequest", FakeXMLHttpRequest);
    FakeXMLHttpRequest.planned = [
      { kind: "error" },
      { kind: "load", status: 500, body: '{"error":{"message":"Temporary chunk failure."}}' },
      { kind: "load", status: 204 },
    ];

    await expect(upload(new File(["data"], "desktop.bin"), {
      target: { url: "http://127.0.0.1:4100/upload", headers: { Authorization: "Bearer token-1" } },
    })).rejects.toThrow("Temporary chunk failure.");

    expect(FakeXMLHttpRequest.requests.map((request) => request.url)).toEqual([
      "http://127.0.0.1:4100/upload",
      "http://127.0.0.1:4100/upload/chunk",
      "http://127.0.0.1:4100/upload/chunk",
    ]);
    expect(FakeXMLHttpRequest.requests.map((request) => request.headers.get("Authorization")))
      .toEqual(["Bearer token-1", "Bearer token-1", "Bearer token-1"]);
    expect(FakeXMLHttpRequest.requests[2].headers.get("X-OpenAIDE-Upload-Cancel")).toBe("true");
  });

  it("logs one metadata-only lifecycle across the chunk fallback", async () => {
    vi.stubGlobal("XMLHttpRequest", FakeXMLHttpRequest);
    FakeXMLHttpRequest.planned = [
      { kind: "error" },
      { kind: "load", status: 200, body: '{"attachment":{"handleId":"attachment-3"}}' },
    ];
    const log = vi.fn();

    await upload(new File(["data"], "private-name.bin"), { log });

    const operationId = log.mock.calls[0][2].operation_id;
    expect(log.mock.calls).toEqual([
      ["info", "file_upload_started", { operation_id: operationId, mode: "single", bytes: 4 }],
      ["info", "file_upload_chunk_fallback", { operation_id: operationId, reason: "transportError" }],
      ["info", "file_upload_completed", {
        operation_id: operationId,
        outcome: "success",
        mode: "chunked",
        attempt_count: 2,
        duration_ms: expect.any(Number),
      }],
    ]);
    expect(JSON.stringify(log.mock.calls)).not.toContain("private-name");
  });

  it("classifies a refused upload without logging the App Server's message", async () => {
    vi.stubGlobal("XMLHttpRequest", FakeXMLHttpRequest);
    FakeXMLHttpRequest.planned = [
      { kind: "load", status: 400, body: '{"error":{"message":"Attachment is not allowed."}}' },
    ];
    const log = vi.fn();

    await expect(upload(new File(["data"], "blocked.bin"), { log })).rejects.toThrow("Attachment is not allowed.");

    expect(log).toHaveBeenLastCalledWith("warn", "file_upload_completed", expect.objectContaining({
      outcome: "failure",
      mode: "single",
      error_kind: "rejected",
      status: 400,
    }));
    expect(JSON.stringify(log.mock.calls)).not.toContain("not allowed");
  });
});
