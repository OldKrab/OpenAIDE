import { AppServerProtocolError } from "@openaide/app-server-client";
import { describe, expect, it } from "vitest";
import { nativeSessionRecoveryKind } from "./appControllerNativeSessions";

describe("nativeSessionRecoveryKind", () => {
  it.each([
    ["nodeJsRequired", "nodeJsRequired"],
    ["unauthorized", "authRequired"],
    ["capabilityUnavailable", "setupRequired"],
    ["internal", "launchFailed"],
  ] as const)("maps %s to %s", (code, expected) => {
    const error = new AppServerProtocolError({
      error: { code, message: "Agent session discovery failed" },
    });
    expect(nativeSessionRecoveryKind(error)).toBe(expected);
  });
});
