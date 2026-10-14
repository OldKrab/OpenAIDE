import { afterEach, expect, it, vi } from "vitest";
import { createDesktopFileViewerContent, desktopPdfRendererDocumentUrl } from "./desktopFileViewer";

afterEach(() => {
  vi.unstubAllGlobals();
});

const read = (endpointUrl: string, maxBytes = 64) => createDesktopFileViewerContent(
  () => ({ kind: "localHttp", endpointUrl, authToken: "token-1" }),
  "client-1",
).read({ handle: "handle-1", maxBytes, operationId: "operation-1" }, new AbortController().signal);

it("reads preview bytes from the current App Server with its bearer token", async () => {
  const fetch = vi.fn(async () => new Response(new Uint8Array([1, 2, 3]), { headers: { "content-length": "3" } }));
  vi.stubGlobal("fetch", fetch);

  const result = await read("http://127.0.0.1:4100/probe");

  expect(result).toEqual({ kind: "bytes", bytes: expect.any(ArrayBuffer) });
  const [url, init] = fetch.mock.calls[0] as unknown as [string, RequestInit];
  const target = new URL(url);
  expect(`${target.origin}${target.pathname}`).toBe("http://127.0.0.1:4100/download");
  expect(Object.fromEntries(target.searchParams)).toEqual({
    clientInstanceId: "client-1",
    fileViewerHandle: "handle-1",
    operationId: "operation-1",
    preview: "1",
  });
  expect(init).toMatchObject({ cache: "no-store", headers: { Authorization: "Bearer token-1" } });
});

it("keeps a paired computer's gateway base path", async () => {
  const fetch = vi.fn(async () => new Response(null, { status: 404 }));
  vi.stubGlobal("fetch", fetch);

  await expect(read("http://127.0.0.1:4200/base/probe/")).resolves.toEqual({ kind: "notFound" });
  expect(new URL((fetch.mock.calls[0] as unknown as [string])[0]).pathname).toBe("/base/download");
});

it("refuses a document above the preview budget before buffering it", async () => {
  vi.stubGlobal("fetch", vi.fn(async () => new Response("x".repeat(8), { headers: { "content-length": "8" } })));

  await expect(read("http://127.0.0.1:4100/probe", 4)).resolves.toEqual({ kind: "tooLarge" });
});

it("reports an endpoint it cannot address as unavailable without a request", async () => {
  const fetch = vi.fn();
  vi.stubGlobal("fetch", fetch);

  await expect(read("http://127.0.0.1:4100/elsewhere")).resolves.toEqual({ kind: "unavailable" });
  expect(fetch).not.toHaveBeenCalled();
});

it("addresses the native renderer scheme the way each webview exposes it", () => {
  expect(desktopPdfRendererDocumentUrl("windows")).toBe("http://openaide-renderer.localhost/pdf.html");
  expect(desktopPdfRendererDocumentUrl("macos")).toBe("openaide-renderer://localhost/pdf.html");
  expect(desktopPdfRendererDocumentUrl("linux")).toBe("openaide-renderer://localhost/pdf.html");
});
