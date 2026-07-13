import { useMemo, useRef } from "react";
import { AsyncOperationOwner } from "../state/asyncOperationOwner";

export function useAppControllerRefs() {
  const asyncOperations = useRef(new AsyncOperationOwner());
  const latestNavigationSessionKey = useRef<string | undefined>(undefined);
  return useMemo(() => ({
    asyncOperations,
    latestNavigationSessionKey,
  }), []);
}

export type AppControllerRefs = ReturnType<typeof useAppControllerRefs>;

/** Invalidates request ownership whose results belong to a replaced App Server process. */
export function invalidateAppControllerReplicaRequests(refs: AppControllerRefs) {
  refs.latestNavigationSessionKey.current = undefined;
  refs.asyncOperations.current.replaceReplica();
}
