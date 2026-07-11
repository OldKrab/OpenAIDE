import { useRef } from "react";
import { defaultAgent } from "@openaide/app-shell-contracts";
import { SnapshotRequestTracker } from "../state/snapshotRequests";

export type NativeSessionSelectionRef = {
  agentId: string;
  workspaceRoot: string;
};

export function useAppControllerRefs() {
  return {
    latestNativeSessionSelection: useRef<NativeSessionSelectionRef>({
      agentId: defaultAgent.id,
      workspaceRoot: "",
    }),
    latestNavigationSessionKey: useRef<string | undefined>(undefined),
    latestOptionsRequestKey: useRef<string | undefined>(undefined),
    latestSessionListRequestId: useRef<number | undefined>(undefined),
    nextSessionListRequestId: useRef(0),
    snapshotRequests: useRef(new SnapshotRequestTracker()),
  };
}

export type AppControllerRefs = ReturnType<typeof useAppControllerRefs>;
