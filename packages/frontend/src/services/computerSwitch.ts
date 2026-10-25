import { useSyncExternalStore } from "react";
import type { ComputersCapability } from "./computers";
import { hasUnsentInput } from "./unsentInput";

export type ComputerSwitchState = {
  /** The row being connected: a computer id, or "" for this device. */
  pending?: string;
  /** The row waiting for the user to agree to drop their unsent input. */
  confirming?: string;
  /** The shell can still end the pending switch. */
  cancellable: boolean;
  /** The last refused switch and the shell's sentence for it. */
  failed?: { id: string; message: string };
};

export type ComputerSwitch = {
  snapshot(): ComputerSwitchState;
  subscribe(listener: () => void): () => void;
  /**
   * Asks the shell to switch; settles when the shell accepted, refused, or
   * cancelled, or when the switch waits for `confirm` because of unsent input.
   */
  request(id?: string): Promise<void>;
  /** Drops the unsent input and goes on with the switch waiting for it. */
  confirm(): Promise<void>;
  /** Keeps the unsent input and stays on the computer in use. */
  decline(): void;
  cancel(): void;
  /** Leaves a refused switch: the user stays on the computer in use. */
  dismiss(): void;
};

/** Survives the shell's page replacement so the reloaded page can close the switch's diagnostics. */
const ARRIVAL_KEY = "openaide.computerSwitchArrival";

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
  let waiting: Attempt | undefined;
  const run = async (attempt: Attempt) => {
    const row = attempt.id ?? "";
    const { operationId, target } = attempt;
    publish({ pending: row, cancellable: Boolean(capability.cancel) });
    try {
      const result = await capability.use(attempt.id, operationId);
      console.info(`computer_switch_completed operation_id=${operationId} target=${target} outcome=${result} duration_ms=${elapsed(attempt)}`);
      if (result === "switched") rememberArrival(attempt);
      // A switched shell replaces this page; until then the row stays pending.
      publish(result === "switched" ? { pending: row, cancellable: false } : idle);
    } catch (cause) {
      console.warn(`computer_switch_completed operation_id=${operationId} target=${target} outcome=refused duration_ms=${elapsed(attempt)}`);
      publish({ cancellable: false, failed: { id: row, message: refusal(cause) } });
    }
  };
  const store: ComputerSwitch = {
    snapshot: () => state,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    async request(id) {
      if (state.pending !== undefined || state.confirming !== undefined) return;
      const row = id ?? "";
      const attempt = {
        id,
        operationId: `computer-switch-${crypto.randomUUID()}`,
        startedAt: performance.now(),
        target: id ? "computer" : "this_device",
      };
      console.info(`computer_switch_started operation_id=${attempt.operationId} target=${attempt.target}`);
      if (hasUnsentInput()) {
        // ADR-0070 asks before connecting rather than after: a refused or
        // cancelled connection then still leaves the input in place.
        waiting = attempt;
        publish({ confirming: row, cancellable: false });
        return;
      }
      await run(attempt);
    },
    async confirm() {
      const attempt = waiting;
      if (!attempt) return;
      waiting = undefined;
      // TODO: drafts are dropped with the page on a confirmed switch. Keep them
      // per computer and Task in device storage so a switch loses nothing (ADR-0070).
      await run(attempt);
    },
    decline() {
      const attempt = waiting;
      if (!attempt) return;
      waiting = undefined;
      console.info(`computer_switch_completed operation_id=${attempt.operationId} target=${attempt.target} outcome=declined duration_ms=${elapsed(attempt)}`);
      publish(idle);
    },
    dismiss() {
      if (!state.failed) return;
      console.info(`computer_switch_dismissed target=${state.failed.id ? "computer" : "this_device"}`);
      publish(idle);
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

type Attempt = { id?: string; operationId: string; startedAt: number; target: string };

const elapsed = (attempt: Attempt) => Math.round(performance.now() - attempt.startedAt);

function rememberArrival(attempt: Attempt) {
  try {
    sessionStorage.setItem(ARRIVAL_KEY, JSON.stringify({ operationId: attempt.operationId, switchedAt: Date.now() }));
  } catch {
    // Diagnostics only; a page without storage simply does not report arrival.
  }
}

/**
 * Closes the diagnostics of a switch that replaced the previous page, once this
 * page first reaches a ready App Server. A shell that moves the page to another
 * origin starts with empty storage; its own diagnostics cover the switch.
 */
export function reportComputerSwitchArrival() {
  let arrival: { operationId?: unknown; switchedAt?: unknown } | undefined;
  try {
    const stored = sessionStorage.getItem(ARRIVAL_KEY);
    sessionStorage.removeItem(ARRIVAL_KEY);
    arrival = stored ? JSON.parse(stored) as typeof arrival : undefined;
  } catch {
    return;
  }
  if (typeof arrival?.operationId !== "string" || typeof arrival.switchedAt !== "number") return;
  const operationId = /^[\w-]{1,80}$/.test(arrival.operationId) ? arrival.operationId : "invalid";
  console.info(`computer_switch_ready operation_id=${operationId} duration_ms=${Math.max(0, Date.now() - arrival.switchedAt)}`);
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
