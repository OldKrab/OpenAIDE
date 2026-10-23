import { useSyncExternalStore } from "react";
import type { ComputersCapability } from "./computers";

export type ComputerSwitchState = {
  /** The row being connected: a computer id, or "" for this device. */
  pending?: string;
  /** The shell can still end the pending switch. */
  cancellable: boolean;
  /** The last refused switch and the shell's sentence for it. */
  failed?: { id: string; message: string };
};

export type ComputerSwitch = {
  snapshot(): ComputerSwitchState;
  subscribe(listener: () => void): () => void;
  /** Asks the shell to switch; settles when the shell accepted, refused, or cancelled. */
  request(id?: string): Promise<void>;
  cancel(): void;
};

const idle: ComputerSwitchState = { cancellable: false };
const switches = new WeakMap<ComputersCapability, ComputerSwitch>();

/**
 * The Frontend's one switch in progress for a shell's computers, shared by the
 * Task navigation and Settings so both show the same pending row and refusal.
 * It is presentation state only: the shell decides and applies the switch.
 */
export function computerSwitch(capability: ComputersCapability): ComputerSwitch {
  const existing = switches.get(capability);
  if (existing) return existing;
  let state = idle;
  const listeners = new Set<() => void>();
  const publish = (next: ComputerSwitchState) => {
    state = next;
    listeners.forEach((listener) => listener());
  };
  const store: ComputerSwitch = {
    snapshot: () => state,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    async request(id) {
      if (state.pending !== undefined) return;
      const row = id ?? "";
      publish({ pending: row, cancellable: Boolean(capability.cancel) });
      const operationId = `computer-switch-${crypto.randomUUID()}`;
      const startedAt = performance.now();
      const target = id ? "computer" : "this_device";
      const elapsed = () => Math.round(performance.now() - startedAt);
      console.info(`computer_switch_started operation_id=${operationId} target=${target}`);
      try {
        const result = await capability.use(id, operationId);
        console.info(`computer_switch_completed operation_id=${operationId} target=${target} outcome=${result} duration_ms=${elapsed()}`);
        // A switched shell replaces this page; until then the row stays pending.
        publish(result === "switched" ? { pending: row, cancellable: false } : idle);
      } catch (cause) {
        console.warn(`computer_switch_completed operation_id=${operationId} target=${target} outcome=refused duration_ms=${elapsed()}`);
        publish({ cancellable: false, failed: { id: row, message: refusal(cause) } });
      }
    },
    cancel() {
      if (state.pending === undefined || !state.cancellable || !capability.cancel) return;
      publish({ ...state, cancellable: false });
      void capability.cancel().catch(() => {
        console.warn("computer_switch_cancel_failed error_kind=shell");
      });
    },
  };
  switches.set(capability, store);
  return store;
}

/** The shell refuses with a sentence for the user; anything else gets a generic one. */
function refusal(cause: unknown) {
  if (typeof cause === "string" && cause) return cause;
  return cause instanceof Error && cause.message ? cause.message : "OpenAIDE could not switch. Try again.";
}

const noSubscription = () => () => undefined;
const idleSnapshot = () => idle;

export function useComputerSwitch(store: ComputerSwitch | undefined): ComputerSwitchState {
  return useSyncExternalStore(store?.subscribe ?? noSubscription, store?.snapshot ?? idleSnapshot, store?.snapshot ?? idleSnapshot);
}
