import { useEffect } from "react";
import { AGENT_REFRESH_ACCOUNT_LIMITS, type AgentId, type BackendConnection } from "@openaide/app-server-client";

/** How often a Task that stays in view asks again. */
const ASK_INTERVAL_MS = 5 * 60_000;
/** Returning to the tab asks at once, but tab hopping must not turn into a request per hop. */
const MIN_ASK_GAP_MS = 60_000;

type AccountLimitsRefreshOptions = {
  /** The Agent of the Task in view; nothing is asked without one. */
  agentId?: string;
  backendConnection?: Pick<BackendConnection, "request">;
};

/**
 * Tells App Server that someone is looking at this Agent's Account Limits: when the Task opens,
 * when the page comes back into view, and periodically while it stays there. App Server owns
 * whether a read is due and publishes the reading with the Agent collection, so this hook keeps
 * no result and treats a failure as nothing to show.
 */
export function useAccountLimitsRefresh({ agentId, backendConnection }: AccountLimitsRefreshOptions) {
  useEffect(() => {
    // Some shells and test hosts expose a window without events; there is no view to follow there.
    if (!backendConnection?.request || !agentId || typeof window === "undefined" || !window.addEventListener) return;
    const ownerDocument = typeof document === "undefined" ? undefined : document;
    let lastAsk: number | undefined;

    const ask = () => {
      if (ownerDocument?.visibilityState === "hidden") return;
      const now = Date.now();
      if (lastAsk !== undefined && now - lastAsk < MIN_ASK_GAP_MS) return;
      lastAsk = now;
      void backendConnection.request(AGENT_REFRESH_ACCOUNT_LIMITS, { agentId: agentId as AgentId }).catch(() => {
        // The limits shown stay as they are; the next ask retries.
      });
    };

    ask();
    const interval = setInterval(ask, ASK_INTERVAL_MS);
    window.addEventListener("focus", ask);
    ownerDocument?.addEventListener("visibilitychange", ask);
    return () => {
      clearInterval(interval);
      window.removeEventListener("focus", ask);
      ownerDocument?.removeEventListener("visibilitychange", ask);
    };
  }, [agentId, backendConnection]);
}
