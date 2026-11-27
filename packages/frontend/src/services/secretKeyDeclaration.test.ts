import { afterEach, describe, expect, it, vi } from "vitest";
import { CLIENT_SECRET_KEYS_CHANGED } from "@openaide/app-server-client";
import type { HostToWebviewMessage } from "@openaide/app-shell-contracts";
import { startSecretKeyDeclaration } from "./secretKeyDeclaration";

function harness(keyIds: string[] | undefined) {
  const listeners = new Set<(message: HostToWebviewMessage) => void>();
  const notify = vi.fn();
  const log = vi.fn();
  const emit = (message: HostToWebviewMessage) => listeners.forEach((listener) => listener(message));
  const postHostMessage = vi.fn((message: { type: string; payload: { requestId: string } }) => {
    if (message.type === "secret.keys.request" && keyIds) {
      queueMicrotask(() => emit({
        type: "secret.keys.result",
        payload: { requestId: message.payload.requestId, keyIds, unknownHolder: true },
      }));
    }
  });
  const declaration = startSecretKeyDeclaration({
    backendConnection: { notify },
    log,
    postHostMessage: postHostMessage as never,
    subscribeHostMessages: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  });
  return { declaration, emit, log, notify };
}

afterEach(() => vi.useRealTimers());

describe("secret key declaration", () => {
  it("sends the shell's key ids to the App Server", async () => {
    const { declaration, notify, log } = harness(["openaide.agent.codex.env.KEY"]);
    await declaration.declare();
    expect(notify).toHaveBeenCalledWith(CLIENT_SECRET_KEYS_CHANGED, {
      keyIds: ["openaide.agent.codex.env.KEY"],
      unknownHolder: true,
    });
    expect(log).toHaveBeenLastCalledWith("secret_keys_declare_completed", expect.objectContaining({
      outcome: "success",
      key_count: 1,
    }));
  });

  it("declares again after a successful secret transaction", async () => {
    const { declaration, emit, notify } = harness(["k"]);
    emit({ type: "secret.transaction.result", payload: { requestId: "r", transactionId: "t", ok: true } });
    await vi.waitFor(() => expect(notify).toHaveBeenCalledTimes(1));
    declaration.dispose();
  });

  it("logs a failure and declares nothing when the shell never answers", async () => {
    vi.useFakeTimers();
    const { declaration, notify, log } = harness(undefined);
    const pending = declaration.declare();
    await vi.advanceTimersByTimeAsync(15_000);
    await pending;
    expect(notify).not.toHaveBeenCalled();
    expect(log).toHaveBeenLastCalledWith("secret_keys_declare_completed", expect.objectContaining({
      outcome: "failure",
    }));
  });
});
