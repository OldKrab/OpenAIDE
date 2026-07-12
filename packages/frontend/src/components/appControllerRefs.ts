import { useMemo, useRef } from "react";
import { defaultAgent } from "@openaide/app-shell-contracts";
import { SnapshotRequestTracker } from "../state/snapshotRequests";

export type NativeSessionSelectionRef = {
  agentId: string;
  workspaceRoot: string;
};

export function useAppControllerRefs() {
  const latestNativeSessionSelection = useRef<NativeSessionSelectionRef>({
    agentId: defaultAgent.id,
    workspaceRoot: "",
  });
  const latestNavigationSessionKey = useRef<string | undefined>(undefined);
  const latestOptionsRequestKey = useRef<string | undefined>(undefined);
  const latestSessionListRequestId = useRef<number | undefined>(undefined);
  // Page state is discarded after authoritative history replacement; request identity must survive it.
  const nextChatPageRequestGeneration = useRef(0);
  const nextSessionListRequestId = useRef(0);
  const snapshotRequests = useRef(new SnapshotRequestTracker());
  return useMemo(() => ({
    latestNativeSessionSelection,
    latestNavigationSessionKey,
    latestOptionsRequestKey,
    latestSessionListRequestId,
    nextChatPageRequestGeneration,
    nextSessionListRequestId,
    snapshotRequests,
  }), []);
}

export type AppControllerRefs = ReturnType<typeof useAppControllerRefs>;

/** Invalidates request ownership whose results belong to a replaced App Server process. */
export function invalidateAppControllerReplicaRequests(refs: AppControllerRefs) {
  refs.latestNavigationSessionKey.current = undefined;
  refs.latestOptionsRequestKey.current = undefined;
  refs.latestSessionListRequestId.current = undefined;
  refs.snapshotRequests.current.beginNavigationChange();
}
