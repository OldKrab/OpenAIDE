import { describe, expect, it } from "vitest";
import type { ToolDetailSnapshot } from "@openaide/app-server-client";
import { mapProtocolToolDetail } from "./appServerProtocolChatMapping";

describe("App Server Protocol Chat mapping", () => {
  it("maps every typed tool content part and nested field value", () => {
    const details: ToolDetailSnapshot = {
      locations: [],
      content: [
        { kind: "text", text: "text" },
        { kind: "diff", path: "src/file.ts", newText: "new" },
        { kind: "terminal", terminalId: "terminal-1" },
        { kind: "image", mediaType: "image/png", dataUrl: "data:image/png;base64,aW1hZ2U=" },
        { kind: "audio", mediaType: "audio/wav", dataUrl: "data:audio/wav;base64,YXVkaW8=" },
        { kind: "resource", uri: "https://example.test/guide", name: "Guide", sizeBytes: 42 },
        { kind: "unsupported", contentType: "resource_blob", mediaType: "application/octet-stream" },
      ],
      input: {
        command: [],
        fields: [{
          name: "filters",
          value: {
            kind: "object",
            fields: [{ name: "limit", value: { kind: "number", value: "10" } }],
          },
        }],
      },
    };

    expect(mapProtocolToolDetail(details)).toEqual({
      locations: [],
      content: [
        { kind: "text", text: "text" },
        { kind: "diff", path: "src/file.ts", old_text: undefined, new_text: "new" },
        { kind: "terminal", terminal_id: "terminal-1" },
        { kind: "image", media_type: "image/png", data_url: "data:image/png;base64,aW1hZ2U=", uri: undefined },
        { kind: "audio", media_type: "audio/wav", data_url: "data:audio/wav;base64,YXVkaW8=" },
        {
          kind: "resource",
          uri: "https://example.test/guide",
          name: "Guide",
          title: undefined,
          description: undefined,
          media_type: undefined,
          size_bytes: 42,
          text: undefined,
        },
        { kind: "unsupported", content_type: "resource_blob", media_type: "application/octet-stream", uri: undefined },
      ],
      input: {
        command: [],
        cwd: undefined,
        query: undefined,
        queries: undefined,
        url: undefined,
        path: undefined,
        fields: [{
          name: "filters",
          value: {
            kind: "object",
            fields: [{ name: "limit", value: { kind: "number", value: "10" } }],
          },
        }],
      },
      output: undefined,
    });
  });
});
