import type { ComputerSwitchResult, ComputersCapability, ComputersSnapshot } from "../../../packages/frontend/src/services/computers";
import type { ConnectionCommand, ConnectionSettings, ConnectionSnapshot } from "../../../packages/frontend/src/services/connectionSettings";
import type { DesktopUpdateCapability } from "../../../packages/frontend/src/services/frontendShell";
import { androidAppUpdates, type AndroidUpdateCommand } from "./androidAppUpdates";
import { androidSystemAppearance } from "./androidSystemAppearance";

/**
 * The phone's controls over one native channel: Settings state, which the phone
 * sends only to its Settings sections; its own updates; and the computers, which
 * it sends to every page so the switcher works anywhere (ADR-0070).
 */
export type AndroidConnectionSettings = ConnectionSettings & {
  appUpdates: DesktopUpdateCapability;
  computers: ComputersCapability;
};

/** The phone connects the chosen computer first (ADR-0070); these carry the page's correlation id and a Cancel. */
type SwitchCommand =
  | { action: "use_computer"; id: string; operationId?: string }
  | { action: "local"; operationId?: string }
  | { action: "cancel_switch" };

/** Forgetting the computer in use moves to this phone first, so it is answered like a switch. */
const SWITCH_ACTIONS = new Set(["use_computer", "local", "cancel_switch", "forget_computer"]);
/** Bounds a switch the phone never answers; the user can Cancel well before it. */
const SWITCH_TIMEOUT_MS = 180_000;

export function androidConnectionSettings(host: Window): AndroidConnectionSettings | undefined {
  if (!/(?:^| )OpenAIDE-Android\/1(?: |$)/.test(host.navigator.userAgent)) return undefined;
  let port: MessagePort | undefined;
  const appearance = androidSystemAppearance(host, color => {
    try { port?.postMessage(JSON.stringify({ type: "appearance", color })); }
    catch { }
  });
  let state: ConnectionSnapshot | undefined;
  let computers: ComputersSnapshot | undefined;
  let sequence = 0;
  const listeners = new Set<() => void>();
  const pending = new Map<number, { send(): void; finish(error?: Error, outcome?: unknown): void }>();
  const unavailable = () => new Error("Phone settings are unavailable. Reopen Settings and try again.");
  const notify = () => listeners.forEach(listener => listener());
  host.addEventListener("message", event => {
    if (event.data !== "openaide:connection-controls:1" || event.ports.length !== 1) return;
    if (event.origin && event.origin !== host.location.origin) return;
    if (port) {
      port.close();
      for (const request of pending.values()) request.finish(unavailable());
    }
    port = event.ports[0];
    port.onmessage = event => {
      try {
        const message = JSON.parse(event.data);
        if (message.type === "result") {
          // A switch the phone refused carries the sentence for the user.
          const refused = typeof message.error === "string" && message.error ? new Error(message.error) : unavailable();
          pending.get(message.id)?.finish(message.accepted ? undefined : refused, message.outcome);
        } else if (message.type === "state" && typeof message.state?.remote === "boolean") {
          state = message.state;
          notify();
        } else if (message.type === "update") {
          updates.receive(message.update);
        } else if (message.type === "computers" && Array.isArray(message.computers)) {
          computers = { device: "phone", computers: message.computers };
          notify();
        }
      } catch { }
    };
    port.start();
    appearance.refresh();
    for (const request of pending.values()) request.send();
  });
  host.addEventListener("pagehide", () => {
    appearance.dispose();
    port?.close();
    port = undefined;
    for (const request of pending.values()) request.finish(unavailable());
  });
  host.addEventListener("pageshow", () => appearance.refresh());
  /** Sends one command; a switch is answered when it ends, which a slow computer or this phone's start can delay. */
  const request = (command: ConnectionCommand | AndroidUpdateCommand | SwitchCommand) => new Promise<unknown>((resolve, reject) => {
    const id = ++sequence;
    const switching = SWITCH_ACTIONS.has(command.action);
    const timeout = host.setTimeout(() => pending.get(id)?.finish(unavailable()), switching ? SWITCH_TIMEOUT_MS : 8000);
    const entry = {
      send: () => {
        try { port?.postMessage(JSON.stringify({ id, command })); }
        catch { entry.finish(unavailable()); }
      },
      finish: (error?: Error, outcome?: unknown) => {
        host.clearTimeout(timeout);
        pending.delete(id);
        if (error) reject(error); else resolve(outcome);
      },
    };
    pending.set(id, entry);
    entry.send();
  });
  const execute = async (command: ConnectionCommand | AndroidUpdateCommand) => { await request(command); };
  const switchTo = async (id: string | undefined, operationId: string | undefined): Promise<ComputerSwitchResult> => {
    const outcome = await request(id ? { action: "use_computer", id, operationId } : { action: "local", operationId });
    // The phone replaces this page after it answers `switched`.
    return outcome === "unchanged" || outcome === "cancelled" ? outcome : "switched";
  };
  const updates = androidAppUpdates(execute);
  const subscribe = (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; };
  return {
    snapshot: () => state,
    subscribe,
    execute,
    appUpdates: updates.capability,
    computers: {
      snapshot: () => computers,
      subscribe,
      use: switchTo,
      cancel: async () => { await request({ action: "cancel_switch" }); },
      // Resolves once forgotten, or once a Cancel kept the computer; forgetting the one in use replaces the page.
      forget: async id => { await request({ action: "forget_computer", id }); },
      openPairing: () => execute({ action: "pair_setup" }),
    },
  };
}
