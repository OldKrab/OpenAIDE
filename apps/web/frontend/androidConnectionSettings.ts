import type { ConnectionCommand, ConnectionSettings, ConnectionSnapshot } from "../../../packages/frontend/src/services/connectionSettings";
import { androidSystemAppearance } from "./androidSystemAppearance";

export function androidConnectionSettings(host: Window): ConnectionSettings | undefined {
  if (!/(?:^| )OpenAIDE-Android\/1(?: |$)/.test(host.navigator.userAgent)) return undefined;
  let port: MessagePort | undefined;
  const appearance = androidSystemAppearance(host, color => {
    try { port?.postMessage(JSON.stringify({ type: "appearance", color })); }
    catch { }
  });
  let state: ConnectionSnapshot | undefined;
  let sequence = 0;
  const listeners = new Set<() => void>();
  const pending = new Map<number, { send(): void; finish(error?: Error): void }>();
  const unavailable = () => new Error("Phone settings are unavailable. Reopen Connection and try again.");
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
          state = { ...message.state, scannedAddress: state?.scannedAddress, scanSequence: state?.scanSequence };
          notify();
        } else if (message.type === "scanned" && state && typeof message.address === "string") {
          state = { ...state, scannedAddress: message.address, scanSequence: (state.scanSequence ?? 0) + 1 };
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
  return {
    snapshot: () => state,
    subscribe: listener => { listeners.add(listener); return () => { listeners.delete(listener); }; },
    execute: (command: ConnectionCommand) => new Promise<void>((resolve, reject) => {
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
    }),
  };
}
