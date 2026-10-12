import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import type { SettingsTabId } from "@openaide/app-shell-contracts";
import type { ConnectionCommand, ConnectionSettings, ConnectionSnapshot } from "../../services/connectionSettings";

export type PhoneConnection = {
  /** Undefined until the phone shell reports its first state. */
  state: ConnectionSnapshot | undefined;
  busy: boolean;
  /** Set when the phone shell did not answer the last command. */
  error: string;
  execute: (command: ConnectionCommand) => Promise<void>;
};

/**
 * The sections that show phone controls. The phone shell answers only while
 * the route names one of them, so this mirrors `SetupPagePolicy.PHONE_SECTIONS`.
 */
export const phoneSections: readonly SettingsTabId[] = ["general", "remote", "troubleshooting"];

const noSubscription = () => () => undefined;
const noSnapshot = () => undefined;

/**
 * The phone shell owns its connection state; every section that shows phone
 * controls renders from the same snapshot. Settings asks for it per section,
 * once the route names that section.
 */
export function usePhoneConnection(capability: ConnectionSettings | undefined): PhoneConnection | undefined {
  const state = useSyncExternalStore(
    capability?.subscribe ?? noSubscription,
    capability?.snapshot ?? noSnapshot,
    capability?.snapshot ?? noSnapshot,
  );
  const [error, setError] = useState("");
  const [sending, setSending] = useState(false);
  const mounted = useRef(true);
  const execute = async (command: ConnectionCommand) => {
    if (!capability) return;
    setError("");
    setSending(true);
    try {
      await capability.execute(command);
    } catch {
      if (mounted.current) setError("Phone settings did not respond. Try again, or reopen OpenAIDE.");
    } finally {
      if (mounted.current) setSending(false);
    }
  };
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);
  if (!capability) return undefined;
  return { state, busy: sending || Boolean(state?.busy), error, execute };
}
