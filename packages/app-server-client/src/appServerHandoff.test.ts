import { describe, expect, it } from "vitest";
import { parseAppServerHandoffConnection } from "./appServerHandoff.js";

describe("parseAppServerHandoffConnection", () => {
  it("accepts local HTTP handoff records with loopback endpoints", () => {
    expect(parseAppServerHandoffConnection(JSON.stringify({
      kind: "localHttp",
      endpointUrl: "http://127.0.0.1:1234/probe",
      authToken: "x".repeat(32),
    }))).toEqual({
      endpointUrl: "http://127.0.0.1:1234/probe",
      authToken: "x".repeat(32),
    });
  });

  it("rejects non-loopback handoff endpoints", () => {
    expect(() => parseAppServerHandoffConnection(JSON.stringify({
      kind: "localHttp",
      endpointUrl: "http://example.com/probe",
      authToken: "x".repeat(32),
    }))).toThrow("loopback");
  });
});
