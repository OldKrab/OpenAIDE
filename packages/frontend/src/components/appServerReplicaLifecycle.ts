import { useCallback, useRef, useState, type Dispatch } from "react";
import type {
  BackendReplicaIdentity,
  ClientSnapshot,
} from "@openaide/app-server-client";
import {
  bindAppServerReplicaEpoch,
  type AppAction,
} from "../state/appReducer";

export type AppServerReplicaTransition = {
  current: BackendReplicaIdentity;
  epoch: number;
  previous?: BackendReplicaIdentity;
  rootChanged: boolean;
};

/** Owns process identity and stamps asynchronous outcomes with their origin epoch. */
export function useAppServerReplicaLifecycle(
  dispatch: Dispatch<AppAction>,
  onReplicaChanged?: (transition: AppServerReplicaTransition) => void,
) {
  const [replicaEpoch, setReplicaEpoch] = useState(0);
  const replicaEpochRef = useRef(0);
  const replicaIdentity = useRef<BackendReplicaIdentity | undefined>(undefined);

  const establishReplica = useCallback((identity: BackendReplicaIdentity) => {
    const previous = replicaIdentity.current;
    const changed = !previous
      || previous.serverId !== identity.serverId
      || previous.stateRootId !== identity.stateRootId;
    replicaIdentity.current = identity;
    if (!changed) return replicaEpochRef.current;
    replicaEpochRef.current += 1;
    const epoch = replicaEpochRef.current;
    onReplicaChanged?.({
      current: identity,
      epoch,
      previous,
      rootChanged: previous !== undefined && previous.stateRootId !== identity.stateRootId,
    });
    setReplicaEpoch(epoch);
    dispatch({ type: "appServer:replica", epoch, stateRootId: identity.stateRootId });
    return epoch;
  }, [dispatch, onReplicaChanged]);

  const dispatchForCurrentReplica = useCallback((action: AppAction) => {
    bindAppServerReplicaEpoch(dispatch, replicaEpochRef.current)(action);
  }, [dispatch]);

  return {
    dispatchForCurrentReplica,
    establishReplica,
    replicaEpoch,
    replicaEpochRef,
    replicaIdentity,
  };
}

export function replicaIdentityFromSnapshot(snapshot: ClientSnapshot): BackendReplicaIdentity {
  return {
    serverId: snapshot.server.serverId,
    stateRootId: snapshot.stateRoot.stateRootId,
  };
}
