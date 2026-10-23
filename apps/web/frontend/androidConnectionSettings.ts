import type { ComputersCapability, ComputersSnapshot } from "../../../packages/frontend/src/services/computers";
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
  const pending = new Map<number, { send(): void; finish(error?: Error): void }>();
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
          pending.get(message.id)?.finish(message.accepted ? undefined : unavailable());
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
  const execute = (command: ConnectionCommand | AndroidUpdateCommand) => new Promise<void>((resolve, reject) => {
    const id = ++sequence;
    const timeout = host.setTimeout(() => pending.get(id)?.finish(unavailable()), 8000);
    const request = {
      send: () => {
        try { port?.postMessage(JSON.stringify({ id, command })); }
        catch { request.finish(unavailable()); }
      },
      finish: (error?: Error) => {
        host.clearTimeout(timeout);
        pending.delete(id);
        if (error) reject(error); else resolve();
      },
    };
    pending.set(id, request);
    request.send();
  });
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
      use: id => execute(id ? { action: "use_computer", id } : { action: "local" }),
      forget: id => execute({ action: "forget_computer", id }),
      openPairing: () => execute({ action: "pair_setup" }),
    },
  };
}
