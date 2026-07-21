import { describe, expect, it } from "vitest";
import { mapProtocolToolDetail } from "./appServerProtocolChatMapping";

describe("task read mapping", () => {
  it("preserves individual web-search queries from lazy tool details", () => {
    const details = mapProtocolToolDetail({
      revision: 0,
      locations: [],
      content: [],
      input: {
        command: [],
        query: "combined query ...",
        queries: ["English weather query", "Русский запрос погоды"],
        fields: [{ name: "type", value: { kind: "string", value: "webSearch" } }],
      },
    });

    expect(details.input?.queries).toEqual(["English weather query", "Русский запрос погоды"]);
  });
});
