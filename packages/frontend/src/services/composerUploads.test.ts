import { DIAGNOSTICS_RECORD_CLIENT_EVENTS, type BackendConnection, type ClientDiagnosticEvent } from "@openaide/app-server-client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { uploadComposerFile } from "./composerUploads";
import { uploadFileOverHttp } from "./fileUploadHttp";
import type { FrontendFileAcquisition } from "./frontendShell";

// The App Shell's own telemetry sink; these tests read the copy forwarded to the App Server.
vi.mock("./hostBridge", () => ({ postHostMessage: vi.fn() }));

type Answer = { kind: "load"; status: number; body: string; contentType: string } | { kind: "error" };

/** Answers every upload request alike; `load` listeners fire synchronously, as the transport allows. */
function answeringXMLHttpRequest(answer: Answer) {
  return class {
    status = 0;
    responseText = "";
    readonly upload = { addEventListener() {} };
    private readonly listeners = new Map<string, () => void>();
    open() {}
    setRequestHeader() {}
    getResponseHeader(name: string) {
      return answer.kind === "load" && name.toLowerCase() === "content-type" ? answer.contentType : null;
    }
    addEventListener(type: string, listener: () => void) {
      this.listeners.set(type, listener);
    }
    send() {
      if (answer.kind === "error") {
        this.listeners.get("error")?.();
        return;
      }
      this.status = answer.status;
      this.responseText = answer.body;
      this.listeners.get("load")?.();
    }
    abort() {}
  };
}

async function forwardedUploadEnd(answer: Answer) {
  vi.stubGlobal("XMLHttpRequest", answeringXMLHttpRequest(answer));
  const request = vi.fn(async (_method: string, _params: { events: ClientDiagnosticEvent[] }) => ({}));
  const files: Extract<FrontendFileAcquisition, { kind: "webUpload" }> = {
    kind: "webUpload",
    upload: (taskId, file, onProgress, signal) => uploadFileOverHttp({
      target: { url: "/__openaide-app-server/upload" },
      taskId,
      file,
      kind: "image",
      clientInstanceId: "client-1",
      onProgress,
      signal,
    }),
  };

  await expect(uploadComposerFile(
    files,
    { request: request as unknown as BackendConnection["request"] },
    "task-1",
    new File(["image bytes"], "private-photo.jpg"),
    { kind: "image", onProgress: vi.fn(), signal: new AbortController().signal },
  )).rejects.toThrow("File upload failed.");
  // timing: mocked — releases the diagnostics forwarder's batch delay.
  await vi.advanceTimersByTimeAsync(1_000);

  const forwarded = request.mock.calls
    .filter(([method]) => method === DIAGNOSTICS_RECORD_CLIENT_EVENTS)
    .flatMap(([, params]) => params.events);
  expect(JSON.stringify(forwarded)).not.toContain("private-photo");
  return forwarded.find((event) => event.event === "composer_upload_completed")?.fields;
}

describe("Composer upload diagnostics", () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("forwards the failure class and HTTP status of a refused upload to the App Server log", async () => {
    vi.useFakeTimers();

    // The Android gateway's answer when it cannot open a stream to the paired computer.
    const fields = await forwardedUploadEnd({
      kind: "load",
      status: 502,
      body: "Bad Gateway",
      contentType: "text/plain; charset=utf-8",
    });

    expect(fields).toMatchObject({ outcome: "failure", error_kind: "rejected", status: 502 });
  });

  it("forwards a transport failure without an HTTP status", async () => {
    vi.useFakeTimers();

    const fields = await forwardedUploadEnd({ kind: "error" });

    expect(fields).toMatchObject({ outcome: "failure", error_kind: "transportError" });
    expect(fields).not.toHaveProperty("status");
  });
});
