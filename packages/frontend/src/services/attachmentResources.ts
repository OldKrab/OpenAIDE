import {
  ATTACHMENT_RELEASE,
  type AttachmentCandidateId,
  type AttachmentHandleId,
  type AttachmentResourceId,
  type BackendConnection,
  type TaskId,
} from "@openaide/app-server-client";
import type { ComposerAttachment } from "../state/composerOptions";
import type { AppState } from "../state/store";

type AttachmentResourceConnection = Partial<Pick<BackendConnection, "request">>;

export type ComposerAttachmentResource = {
  taskId: string;
  handleId: AttachmentHandleId;
};

export type ComposerAttachmentAdoption = {
  generation: number;
  stateRootGeneration: number;
  taskId: string;
};

export type ComposerAttachmentAdoptionStatus = "current" | "expired" | "replacedReplica";

export type ComposerAttachmentResourceFrame = {
  acceptedUserMessageIds: Map<string, string | undefined>;
  acceptsAdoptions: boolean;
  retained: ComposerAttachmentResource[];
  mountedTaskId?: string;
  protected: Set<string>;
  taskSurfaceMounted: boolean;
};

/** Derives resolver ownership from the one composer currently mounted by the Task surface. */
export function composerAttachmentResourceFrame(
  state: AppState,
  taskSurfaceMounted: boolean,
  newTaskId?: string,
): ComposerAttachmentResourceFrame {
  const acceptedUserMessageIds = new Map(
    Object.entries(state.taskInputs).map(([taskId, input]) => [taskId, input.acceptedUserMessageId]),
  );
  const protectedResources = new Set<string>();
  for (const [taskId, input] of Object.entries(state.taskInputs)) {
    addResourceKeys(protectedResources, taskId, input.pending?.context ?? []);
  }
  const snapshotTaskId = state.snapshot?.task.task_id;
  if (snapshotTaskId && !state.snapshot?.task.has_messages) {
    addResourceKeys(protectedResources, snapshotTaskId, state.newTask.pending?.context ?? []);
  }

  const retainedResources = new Map<string, ComposerAttachmentResource>();
  if (newTaskId) {
    for (const resource of resourcesFromAttachments(
      newTaskId,
      state.taskInputs[newTaskId]?.context ?? [],
    )) {
      retainedResources.set(resourceKey(resource), resource);
    }
  }

  if (!taskSurfaceMounted || !snapshotTaskId || !state.snapshot) {
    return {
      acceptedUserMessageIds,
      acceptsAdoptions: taskSurfaceMounted && !state.newTask.submitting,
      retained: [...retainedResources.values()],
      // The controller cache is the stable New Task identity while replica
      // ingestion may temporarily omit the same snapshot from reducer state.
      mountedTaskId: taskSurfaceMounted ? newTaskId : undefined,
      protected: protectedResources,
      taskSurfaceMounted,
    };
  }
  const taskInput = state.taskInputs[snapshotTaskId];
  const mountedContext = state.snapshot.task.has_messages
    ? taskInput?.context ?? []
    : state.newTask.submitting
      ? state.newTask.pending?.context ?? taskInput?.pending?.context ?? []
      : taskInput?.context ?? state.newTask.context;
  for (const [taskId, input] of Object.entries(state.taskInputs)) {
    for (const resource of resourcesFromAttachments(taskId, input.context)) {
      retainedResources.set(resourceKey(resource), resource);
    }
  }
  for (const resource of resourcesFromAttachments(snapshotTaskId, mountedContext)) {
    retainedResources.set(resourceKey(resource), resource);
  }
  return {
    acceptedUserMessageIds,
    acceptsAdoptions: state.snapshot.task.has_messages
      ? taskInput?.pending === undefined
      : !state.newTask.submitting && taskInput?.pending === undefined,
    retained: [...retainedResources.values()],
    mountedTaskId: snapshotTaskId,
    protected: protectedResources,
    taskSurfaceMounted,
  };
}

type OwnedAttachmentResource = ComposerAttachmentResource & {
  acceptedUserMessageId?: string;
};

/** Owns transient handles across composer render, submission, and unmount transitions. */
export class ComposerAttachmentResourceOwner {
  private acceptedUserMessageIds = new Map<string, string | undefined>();
  private adoptionGeneration = 0;
  private adoptionsLocked = true;
  private disposed = false;
  private readonly foreignResources = new Set<string>();
  private mountedTaskId: string | undefined;
  private readonly owned = new Map<string, OwnedAttachmentResource>();
  private stateRootGeneration = 0;
  private taskSurfaceMounted = false;

  constructor(private readonly dependencies: {
    isProtected?: (resource: ComposerAttachmentResource) => boolean;
    release: (taskId: string, handleIds: AttachmentHandleId[]) => void;
  }) {}

  /** Hands the mounted New Task composer to the current controller lease before React commits. */
  claimNewTaskController(taskId: string) {
    if (this.disposed || !this.taskSurfaceMounted || this.adoptionsLocked) return false;
    if (this.mountedTaskId !== taskId) this.adoptionGeneration += 1;
    this.mountedTaskId = taskId;
    return true;
  }

  beginAdoption(taskId: string): ComposerAttachmentAdoption | undefined {
    if (this.disposed || this.adoptionsLocked || this.mountedTaskId !== taskId) {
      console.warn("[OpenAIDE] Composer attachment adoption unavailable", {
        adoptionsLocked: this.adoptionsLocked,
        disposed: this.disposed,
        mountedTaskId: this.mountedTaskId,
        requestedTaskId: taskId,
        taskSurfaceMounted: this.taskSurfaceMounted,
      });
      return undefined;
    }
    return {
      generation: this.adoptionGeneration,
      stateRootGeneration: this.stateRootGeneration,
      taskId,
    };
  }

  adopt(resource: ComposerAttachmentResource, adoption?: ComposerAttachmentAdoption) {
    const adoptionStatus = adoption ? this.adoptionStatus(adoption) : undefined;
    if (adoptionStatus === "replacedReplica") {
      // The opaque Task/handle pair may now identify unrelated state. Forget the
      // late response without sending cleanup into the replacement root.
      return false;
    }
    const adoptionExpired = adoptionStatus === "expired" || (
      adoption !== undefined && adoption.taskId !== resource.taskId
    );
    if (this.disposed || this.adoptionsLocked || adoptionExpired || this.mountedTaskId !== resource.taskId) {
      this.dependencies.release(resource.taskId, [resource.handleId]);
      return false;
    }
    const key = resourceKey(resource);
    this.foreignResources.delete(key);
    if (this.owned.has(key)) return true;
    this.owned.set(key, {
      ...resource,
      acceptedUserMessageId: this.acceptedUserMessageIds.get(resource.taskId),
    });
    return true;
  }

  adoptionStatus(adoption: ComposerAttachmentAdoption): ComposerAttachmentAdoptionStatus {
    if (adoption.stateRootGeneration !== this.stateRootGeneration) return "replacedReplica";
    if (
      this.disposed
      || this.adoptionsLocked
      || adoption.generation !== this.adoptionGeneration
      || adoption.taskId !== this.mountedTaskId
    ) return "expired";
    return "current";
  }

  /** Forgets resolver ids without issuing release requests against a replacement process. */
  replaceReplica() {
    this.stateRootGeneration += 1;
    this.adoptionGeneration += 1;
    this.acceptedUserMessageIds.clear();
    this.adoptionsLocked = true;
    this.mountedTaskId = undefined;
    for (const key of this.owned.keys()) this.foreignResources.add(key);
    this.owned.clear();
    this.taskSurfaceMounted = false;
  }

  replaceStateRoot() {
    this.replaceReplica();
  }

  /** Closes the adoption gate synchronously before task/send can race a selection response. */
  lockAdoptions() {
    this.adoptionGeneration += 1;
    this.adoptionsLocked = true;
  }

  release(resource: ComposerAttachmentResource) {
    this.releaseAll([resource]);
  }

  /** Releases every transient resource owned by a discarded New Task. */
  releaseTask(taskId: string) {
    this.releaseAll([...this.owned.values()].filter((resource) => resource.taskId === taskId));
  }

  releaseAll(resources: ComposerAttachmentResource[]) {
    const releases = new Map<string, Set<AttachmentHandleId>>();
    for (const resource of resources) {
      if (this.foreignResources.has(resourceKey(resource))) continue;
      if (this.dependencies.isProtected?.(resource) === true) continue;
      this.owned.delete(resourceKey(resource));
      const handles = releases.get(resource.taskId) ?? new Set<AttachmentHandleId>();
      handles.add(resource.handleId);
      releases.set(resource.taskId, handles);
    }
    for (const [taskId, handles] of releases) {
      this.dependencies.release(taskId, [...handles]);
    }
  }

  dispose(frame: ComposerAttachmentResourceFrame) {
    this.reconcile({
      ...frame,
      acceptsAdoptions: false,
      retained: [],
      mountedTaskId: undefined,
      taskSurfaceMounted: false,
    });
    // Reconciliation above releases anything no longer retained by the mounted composer.
    this.owned.clear();
    this.disposed = true;
  }

  reconcile(frame: ComposerAttachmentResourceFrame) {
    if (this.mountedTaskId !== frame.mountedTaskId) this.adoptionGeneration += 1;
    if (!this.adoptionsLocked && !frame.acceptsAdoptions) this.adoptionGeneration += 1;
    this.acceptedUserMessageIds = frame.acceptedUserMessageIds;
    this.adoptionsLocked = !frame.acceptsAdoptions;
    this.mountedTaskId = frame.mountedTaskId;
    this.taskSurfaceMounted = frame.taskSurfaceMounted;
    const retainedKeys = new Set(frame.retained.map(resourceKey));
    for (const resource of frame.retained) {
      const key = resourceKey(resource);
      if (this.foreignResources.has(key)) continue;
      if (this.owned.has(key)) continue;
      this.owned.set(key, {
        ...resource,
        acceptedUserMessageId: frame.acceptedUserMessageIds.get(resource.taskId),
      });
    }

    const releases = new Map<string, AttachmentHandleId[]>();
    for (const [key, resource] of this.owned) {
      if (
        retainedKeys.has(key)
        || frame.protected.has(key)
        || this.dependencies.isProtected?.(resource) === true
      ) continue;
      const acceptedUserMessageId = frame.acceptedUserMessageIds.get(resource.taskId);
      if (
        acceptedUserMessageId !== undefined
        && acceptedUserMessageId !== resource.acceptedUserMessageId
      ) {
        // task/send already consumed this resolver resource. A late release must
        // not race an accepted send whose response just cleared the composer.
        this.owned.delete(key);
        continue;
      }
      this.owned.delete(key);
      const handles = releases.get(resource.taskId) ?? [];
      handles.push(resource.handleId);
      releases.set(resource.taskId, handles);
    }
    for (const [taskId, handleIds] of releases) {
      this.dependencies.release(taskId, handleIds);
    }
  }
}

/** Abandons every resolver-backed row while preserving one batch request per Task. */
export function releaseComposerAttachments({
  attachmentResources,
  attachments,
  backendConnection,
  taskId,
}: {
  attachmentResources?: ComposerAttachmentResourceOwner;
  attachments: ComposerAttachment[];
  backendConnection: AttachmentResourceConnection | undefined;
  taskId: string;
}) {
  const resources = resourcesFromAttachments(taskId, attachments);
  if (attachmentResources) {
    attachmentResources.releaseAll(resources);
    return;
  }
  releaseAttachmentResources(
    backendConnection,
    taskId,
    resources.map((resource) => attachmentHandleResource(resource.handleId)),
  );
}

/** Releases transient App Server attachment resources without turning cleanup into UI noise. */
export function releaseAttachmentResources(
  backendConnection: AttachmentResourceConnection | undefined,
  taskId: string,
  resources: AttachmentResourceId[],
) {
  if (!resources.length || !backendConnection?.request) return;
  void backendConnection.request(ATTACHMENT_RELEASE, {
    taskId: taskId as TaskId,
    resources,
  }).catch(() => undefined);
}

export function attachmentHandleResource(id: AttachmentHandleId): AttachmentResourceId {
  return { kind: "handle", id };
}

export function attachmentCandidateResource(id: AttachmentCandidateId): AttachmentResourceId {
  return { kind: "candidate", id };
}

export function attachmentResourceKey(taskId: string, handleId: AttachmentHandleId) {
  return `${taskId}\u0000${handleId}`;
}

function resourceKey(resource: ComposerAttachmentResource) {
  return attachmentResourceKey(resource.taskId, resource.handleId);
}

function resourcesFromAttachments(taskId: string, attachments: ComposerAttachment[]) {
  return attachments.flatMap((attachment) => attachment.app_server_handle_id
    ? [{ taskId, handleId: attachment.app_server_handle_id }]
    : []);
}

function addResourceKeys(target: Set<string>, taskId: string, attachments: ComposerAttachment[]) {
  for (const resource of resourcesFromAttachments(taskId, attachments)) {
    target.add(resourceKey(resource));
  }
}
