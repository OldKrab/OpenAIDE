import { describe, expect, it, vi } from "vitest";
import { AppServerProtocolError, TERMINALS_CLOSE, TERMINALS_OPEN } from "@openaide/app-server-client";
import { createTerminalIntents } from "./terminalIntents";

const refused = (code: "conflict" | "notFound") => new AppServerProtocolError({ error: { code, message: "private detail" } });

describe("Terminal intents", () => {
  it("opens with the target and size and logs metadata only", async () => {
    const events: string[] = [];
    const request = vi.fn().mockResolvedValue({ terminal: { terminalId: "t1" }, attach: { terminalId: "t1", token: "secret", path: "/terminal" } });
    const intents = createTerminalIntents({ request } as never, (_level, event, fields) => events.push(`${event} ${JSON.stringify(fields)}`));
    await intents.open({ kind: "project", projectId: "p1" as never }, { cols: 80, rows: 24 });
    expect(request).toHaveBeenCalledWith(TERMINALS_OPEN, { target: { kind: "project", projectId: "p1" }, cols: 80, rows: 24 }, expect.anything());
    expect(events.map((event) => event.split(" ")[0])).toEqual(["terminal_intent_started", "terminal_intent_completed"]);
    expect(events.join("\n")).not.toMatch(/secret|p1/);
  });

  it("reports a running program and a vanished Terminal as outcomes", async () => {
    const events: string[] = [];
    const request = vi.fn()
      .mockRejectedValueOnce(refused("conflict"))
      .mockRejectedValueOnce(refused("notFound"))
      .mockResolvedValueOnce({});
    const intents = createTerminalIntents({ request } as never, (_level, event, fields) => events.push(`${event} ${JSON.stringify(fields)}`));
    await expect(intents.close("t1", false)).resolves.toBe("busy");
    await expect(intents.close("t1", false)).resolves.toBe("gone");
    await expect(intents.close("t1", true)).resolves.toBe("closed");
    expect(request).toHaveBeenLastCalledWith(TERMINALS_CLOSE, { terminalId: "t1", force: true }, expect.anything());
    expect(events.join("\n")).toMatch(/"error_kind":"conflict"/);
    expect(events.join("\n")).not.toMatch(/private detail/);
  });
});
