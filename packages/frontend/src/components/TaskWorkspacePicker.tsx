import { Fragment, useEffect, useMemo, useState } from "react";
import {
  ArrowLeft,
  CircleAlert,
  Copy,
  FolderOpen,
  FolderRoot,
  GitBranch,
  LockKeyhole,
  MoreHorizontal,
  Pencil,
  Plus,
  RefreshCw,
  Search,
  Trash2,
  X,
} from "lucide-react";
import type {
  WorktreeBaseSelection,
  WorktreeRemovalPreflight,
  WorktreeRepositorySnapshot,
  WorktreeOperationSnapshot,
  WorktreeSummary,
} from "@openaide/app-server-client";
import type { TaskSummary } from "@openaide/app-shell-contracts";
import type { NewTaskViewIntents } from "./NewTaskView";
import type { ProjectOption } from "../state/composerOptions";
import { relativeTime } from "./taskSurfaceHelpers";
import { AgentIcon } from "./AgentIcon";

export function TaskWorkspacePicker({
  initialMode = "choose",
  intents,
  managementOnly = false,
  onClose,
  onUseForNewTask,
  project,
  repository,
  selectedWorktreeId,
  tasks,
}: {
  initialMode?: "choose" | "manage";
  intents: NewTaskViewIntents;
  managementOnly?: boolean;
  onClose: () => void;
  onUseForNewTask?: () => void;
  project: ProjectOption;
  repository?: WorktreeRepositorySnapshot;
  selectedWorktreeId?: string;
  tasks: TaskSummary[];
}) {
  const [mode, setMode] = useState<"choose" | "create" | "manage">(initialMode);
  const [createOrigin, setCreateOrigin] = useState<"choose" | "manage">(
    initialMode === "manage" ? "manage" : "choose",
  );
  const [recreateTarget, setRecreateTarget] = useState<WorktreeSummary>();
  const [query, setQuery] = useState("");
  const [explainedUnavailableId, setExplainedUnavailableId] = useState<string>();
  const [managementTasks, setManagementTasks] = useState(tasks);
  const [managementTasksError, setManagementTasksError] = useState<string>();
  // Forgotten entries stay in the projection only to decorate historical Tasks.
  const worktrees = (repository?.worktrees ?? []).filter((worktree) => !worktree.forgotten);
  const projectRoot = worktrees.find((worktree) => isProjectRoot(worktree, project));
  const narrowManagement = typeof window !== "undefined" && window.matchMedia("(max-width: 760px)").matches;
  const [selectedManagementId, setSelectedManagementId] = useState<string | undefined>(
    narrowManagement ? undefined : projectRoot?.worktreeId,
  );
  const selectedManagement = worktrees.find((item) => item.worktreeId === selectedManagementId);
  useEffect(() => {
    if (selectedManagementId && !selectedManagement) {
      setSelectedManagementId(narrowManagement ? undefined : projectRoot?.worktreeId);
    }
  }, [narrowManagement, projectRoot?.worktreeId, selectedManagement, selectedManagementId]);
  useEffect(() => setManagementTasks(tasks), [tasks]);
  useEffect(() => {
    if (mode !== "manage" || !intents.loadProjectTasks) return;
    let active = true;
    setManagementTasksError(undefined);
    void intents.loadProjectTasks(project.projectId).then((loaded) => {
      if (active) setManagementTasks(loaded);
    }).catch((cause) => {
      if (active) setManagementTasksError(cause instanceof Error ? cause.message : "Unable to load linked tasks.");
    });
    return () => { active = false; };
  }, [intents.loadProjectTasks, mode, project.projectId]);

  if (mode === "create") {
    return (
      <CreateWorktreePanel
        intents={intents}
        onBack={() => setMode(createOrigin)}
        onCreated={(created) => {
          if (createOrigin === "choose") {
            selectWorkspace(intents, project, created);
            setMode("choose");
          } else {
            setMode("manage");
          }
        }}
        project={project}
        recreate={recreateTarget}
        repository={repository}
      />
    );
  }

  if (mode === "manage") {
    return (
      <WorktreeManagement
        compact={narrowManagement}
        intents={intents}
        onBack={() => managementOnly ? onClose() : setMode("choose")}
        onCreate={() => { setRecreateTarget(undefined); setCreateOrigin("manage"); setMode("create"); }}
        onRecreate={(worktree) => { setRecreateTarget(worktree); setCreateOrigin("manage"); setMode("create"); }}
        onUseForNewTask={onUseForNewTask}
        project={project}
        repository={repository}
        selected={selectedManagement}
        selectedTaskWorktreeId={selectedWorktreeId}
        select={setSelectedManagementId}
        tasks={managementTasks}
        tasksError={managementTasksError}
      />
    );
  }

  const normalizedQuery = query.trim().toLocaleLowerCase();
  const filtered = worktrees.filter((worktree) => !normalizedQuery
    || `${worktree.name} ${headLabel(worktree)}`.toLocaleLowerCase().includes(normalizedQuery));

  return (
    <div className="task-workspace-popover" role="dialog" aria-label="Task workspace">
      <header className="task-workspace-popover-header">
        <span><strong>Task workspace</strong><small>Choose the folder where this task will run.</small></span>
        <button aria-label="Close task workspace" onClick={onClose} type="button"><X size={15} /></button>
      </header>
      {worktrees.length > 7 ? (
        <label className="task-workspace-search">
          <Search size={13} />
          <input aria-label="Search worktrees" onChange={(event) => setQuery(event.target.value)} placeholder="Search worktrees" value={query} />
        </label>
      ) : null}
      <div className="task-workspace-list-wrap">
        <div className="task-workspace-list" role="listbox">
          {filtered.map((worktree) => {
            const root = isProjectRoot(worktree, project);
            const selected = root ? !selectedWorktreeId : selectedWorktreeId === worktree.worktreeId;
            const available = worktree.availability === "available";
            const reasonOpen = !available && explainedUnavailableId === worktree.worktreeId;
            const reasonId = `worktree-unavailable-${worktree.worktreeId}`;
            return (
              <Fragment key={worktree.worktreeId}>
                <button
                  aria-controls={!available ? reasonId : undefined}
                  aria-expanded={!available ? reasonOpen : undefined}
                  aria-label={!available ? `${root ? "Project root" : worktree.name}, unavailable. Show reason` : undefined}
                  aria-selected={selected}
                  className={`${selected ? "selected " : ""}${!available ? "unavailable" : ""}`.trim()}
                  onClick={() => {
                    if (!available) {
                      setExplainedUnavailableId((current) => current === worktree.worktreeId ? undefined : worktree.worktreeId);
                      return;
                    }
                    selectWorkspace(intents, project, worktree);
                    onClose();
                  }}
                  role="option"
                  type="button"
                >
                  {root ? <FolderRoot size={14} /> : <GitBranch size={14} />}
                  <span><strong>{root ? "Project root" : worktree.name}</strong><small>{chooserMeta(worktree)}</small></span>
                </button>
                {reasonOpen ? <p className="task-workspace-option-reason" id={reasonId} role="status">
                  <CircleAlert size={13} />
                  <span><strong>Unavailable</strong><small>{worktree.availabilityReason ?? "This worktree cannot currently be used."}</small></span>
                </p> : null}
              </Fragment>
            );
          })}
          {!filtered.length ? <p className="task-workspace-empty">No matching worktrees.</p> : null}
        </div>
      </div>
      <footer className="task-workspace-actions">
        <button onClick={() => { setRecreateTarget(undefined); setCreateOrigin("choose"); setMode("create"); }} type="button"><Plus size={14} />New worktree</button>
        <button onClick={() => setMode("manage")} type="button"><MoreHorizontal size={14} />Manage worktrees</button>
      </footer>
      {!repository && project.worktreeError ? <p className="task-workspace-error">{project.worktreeError}</p> : null}
      {!repository && !project.worktreeError ? <p className="task-workspace-empty">Worktrees are unavailable for this Project.</p> : null}
      {projectRoot ? null : null}
    </div>
  );
}

function CreateWorktreePanel({ intents, onBack, onCreated, project, recreate, repository }: {
  intents: NewTaskViewIntents;
  onBack: () => void;
  onCreated: (worktree: WorktreeSummary) => void;
  project: ProjectOption;
  recreate?: WorktreeSummary;
  repository?: WorktreeRepositorySnapshot;
}) {
  const [name, setName] = useState(recreate?.name ?? "");
  const [branch, setBranch] = useState(recreate?.head.kind === "branch" ? recreate.head.name : "");
  const [branchEdited, setBranchEdited] = useState(Boolean(recreate));
  const [createBranch, setCreateBranch] = useState(recreate?.head.kind === "branch");
  const [baseKey, setBaseKey] = useState("head");
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<WorktreeOperationSnapshot>();
  const [error, setError] = useState<string>();
  const bases = repository?.bases ?? [];
  const branchCollision = createBranch && bases.some((item) => item.kind === "localBranch" && item.name === branch.trim());
  const base = baseKey === "head"
    ? ({ kind: "currentHead" } satisfies WorktreeBaseSelection)
    : ({ kind: "localBranch", name: baseKey } satisfies WorktreeBaseSelection);
  const changeName = (value: string) => {
    setName(value);
    if (!branchEdited) {
      const existing = bases.flatMap((item) => item.kind === "localBranch" ? [item.name] : []);
      setBranch(deriveBranchName(value, existing));
    }
  };
  return (
    <div className="task-workspace-popover task-workspace-create" role="dialog" aria-label="New worktree">
      <header className="task-workspace-popover-header">
        <button aria-label="Back to task workspace" onClick={onBack} type="button"><ArrowLeft size={15} /></button>
        <span><strong>{recreate ? "Recreate worktree" : "New worktree"}</strong><small>{project.label}</small></span>
      </header>
      <div className="task-workspace-form">
        {recreate ? <p className="task-workspace-recreate-name"><strong>{recreate.name}</strong><small>{recreate.path}</small></p> : <label>Name<input autoFocus onChange={(event) => changeName(event.target.value)} placeholder="Worktree support" value={name} /></label>}
        <label>Base revision<select onChange={(event) => setBaseKey(event.target.value)} value={baseKey}>
          <option value="head">{baseLabel(bases[0])}</option>
          {bases.filter((item) => item.kind === "localBranch").map((item) => <option key={item.name} value={item.name}>{item.name}</option>)}
        </select></label>
        <label className="task-workspace-checkbox"><input checked={createBranch} onChange={(event) => setCreateBranch(event.target.checked)} type="checkbox" />Create a branch</label>
        {createBranch ? <label>Branch name<input aria-invalid={branchCollision || undefined} onChange={(event) => { setBranchEdited(true); setBranch(event.target.value); }} value={branch} />{branchCollision ? <small className="task-workspace-field-error">A local branch with this name already exists.</small> : null}</label> : <small className="task-workspace-form-hint">Leave off for a detached worktree.</small>}
        <p className="task-workspace-dirty-note"><CircleAlert size={13} />New worktrees start from committed files only.</p>
        {busy ? <div className="task-workspace-create-progress" role="status"><RefreshCw size={14} /><span><strong>{progress?.stage ?? (recreate ? "Recreating worktree" : "Creating worktree")}</strong><small>{worktreeProgressLabel(progress)}</small></span></div> : null}
        {error ? <p className="task-workspace-error">{error}</p> : null}
      </div>
      <footer className="task-workspace-form-actions">
        <button disabled={busy} onClick={onBack} type="button">Cancel</button>
        <button className="primary" disabled={busy || !name.trim() || !repository || (createBranch && (!branch.trim() || branchCollision))} onClick={async () => {
          setBusy(true); setError(undefined);
          try {
            const created = recreate
              ? await intents.recreateWorktree(project, recreate.worktreeId, { base, branch: createBranch ? branch.trim() : undefined }, setProgress)
              : await intents.createWorktree(project, { name: name.trim(), base, branch: createBranch ? branch.trim() : undefined }, setProgress);
            onCreated(created);
          } catch (cause) {
            setError(cause instanceof Error ? cause.message : "Unable to create worktree.");
          } finally { setBusy(false); }
        }} type="button">{busy ? "Preparing…" : recreate ? "Recreate worktree" : "Create worktree"}</button>
      </footer>
    </div>
  );
}

function worktreeProgressLabel(progress?: WorktreeOperationSnapshot) {
  if (!progress) return "Git is preparing the folder and local setup.";
  if (progress.totalFiles !== undefined && progress.completedFiles !== undefined) {
    const bytes = progress.totalBytes ? ` · ${formatBytes(progress.completedBytes ?? 0)} of ${formatBytes(progress.totalBytes)}` : "";
    return `${progress.completedFiles} of ${progress.totalFiles} files${bytes}`;
  }
  return progress.state === "queued" ? "Waiting for another worktree operation." : "Git is preparing the folder and local setup.";
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 102.4) / 10} KB`;
  return `${Math.round(bytes / (1024 * 102.4)) / 10} MB`;
}

function WorktreeManagement({ compact, intents, onBack, onCreate, onRecreate, onUseForNewTask, project, repository, selected, selectedTaskWorktreeId, select, tasks, tasksError }: {
  compact: boolean;
  intents: NewTaskViewIntents;
  onBack: () => void;
  onCreate: () => void;
  onRecreate: (worktree: WorktreeSummary) => void;
  onUseForNewTask?: () => void;
  project: ProjectOption;
  repository?: WorktreeRepositorySnapshot;
  selected?: WorktreeSummary;
  selectedTaskWorktreeId?: string;
  select: (id: string | undefined) => void;
  tasks: TaskSummary[];
  tasksError?: string;
}) {
  const [preflight, setPreflight] = useState<WorktreeRemovalPreflight>();
  const [error, setError] = useState<string>();
  const [linkedQuery, setLinkedQuery] = useState("");
  const [editingName, setEditingName] = useState(false);
  const [nameDraft, setNameDraft] = useState(selected?.name ?? "");
  const [renaming, setRenaming] = useState(false);
  const visibleWorktrees = (repository?.worktrees ?? []).filter((worktree) => !worktree.forgotten);
  const projectRoot = visibleWorktrees.find((worktree) => isProjectRoot(worktree, project));
  useEffect(() => {
    setEditingName(false);
    setNameDraft(selected?.name ?? "");
  }, [selected?.name, selected?.worktreeId]);
  const linked = useMemo(() => tasks.filter((task) => selected
    && (isProjectRoot(selected, project) ? !task.worktree_id && task.project_id === project.projectId : task.worktree_id === selected.worktreeId)), [project, selected, tasks]);
  const normalizedLinkedQuery = linkedQuery.trim().toLocaleLowerCase();
  const visibleLinked = normalizedLinkedQuery
    ? linked.filter((task) => task.title.toLocaleLowerCase().includes(normalizedLinkedQuery))
    : linked;
  const saveName = async () => {
    if (!repository || !selected || !nameDraft.trim() || renaming) return;
    setRenaming(true); setError(undefined);
    try {
      await intents.renameWorktree(repository.repositoryId, selected.worktreeId, nameDraft.trim());
      setEditingName(false);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Unable to rename worktree.");
    } finally {
      setRenaming(false);
    }
  };
  return (
    <section className="worktree-management" aria-label="Worktrees">
      <header className="worktree-management-toolbar">
        <button aria-label="Back" onClick={() => compact && selected ? select(undefined) : onBack()} type="button"><ArrowLeft size={15} /></button>
        <strong>Worktrees</strong><small>{project.label}</small>
        <button onClick={onCreate} type="button"><Plus size={14} />New worktree</button>
        <button aria-label="Refresh worktrees" onClick={() => void intents.refreshWorktrees(project)} type="button"><RefreshCw size={14} /></button>
      </header>
      <div className="worktree-management-body">
        <nav aria-label="Worktree list">
          {visibleWorktrees.map((worktree) => (
            <button className={selected?.worktreeId === worktree.worktreeId ? "selected" : ""} key={worktree.worktreeId} onClick={() => { select(worktree.worktreeId); setPreflight(undefined); setLinkedQuery(""); }} type="button">
              {worktree.availability === "unavailable" ? <CircleAlert size={14} /> : isProjectRoot(worktree, project) ? <FolderRoot size={14} /> : <GitBranch size={14} />}
              <span><strong>{isProjectRoot(worktree, project) ? "Project root" : worktree.name}</strong><small>{headLabel(worktree)}</small></span>
              {worktree.availability === "unavailable" ? <em className="unavailable">Unavailable</em> : worktree.lockedReason ? <em><LockKeyhole size={11} />Locked</em> : worktree.runningTaskCount ? <em>{worktree.runningTaskCount} running</em> : worktree.linkedTaskCount ? <em>{worktree.linkedTaskCount} tasks</em> : null}
            </button>
          ))}
        </nav>
        {selected ? <article className="worktree-detail">
          <header>{editingName ? <div className="worktree-name-editor"><input aria-label="Worktree name" autoFocus onChange={(event) => setNameDraft(event.target.value)} onKeyDown={(event) => {
            if (event.key === "Escape") { setEditingName(false); setNameDraft(selected.name); }
            if (event.key === "Enter") void saveName();
          }} value={nameDraft} /><button aria-label="Save worktree name" disabled={renaming || !nameDraft.trim()} onClick={() => void saveName()} type="button"><CheckIcon /></button><button aria-label="Cancel rename" disabled={renaming} onClick={() => { setEditingName(false); setNameDraft(selected.name); }} type="button"><X size={14} /></button></div> : <><span><strong>{isProjectRoot(selected, project) ? "Project root" : selected.name}</strong><small>{headLabel(selected)}</small></span>{isProjectRoot(selected, project) ? null : <button aria-label="Rename worktree" onClick={() => setEditingName(true)} type="button"><Pencil size={14} /></button>}</>}</header>
          {selected.availability === "unavailable" || selected.lockedReason ? <div className={`worktree-state-notice ${selected.availability}`}><CircleAlert size={14} /><span><strong>{selected.availability === "unavailable" ? "Workspace unavailable" : "Locked"}</strong><small>{selected.availabilityReason ?? selected.lockedReason}</small></span><button onClick={() => void intents.refreshWorktrees(project)} type="button"><RefreshCw size={13} />Refresh</button>{selected.availability === "unavailable" ? <button onClick={() => onRecreate(selected)} type="button">Recreate</button> : null}</div> : null}
          {selected.availability === "available" ? <button className="worktree-new-task" onClick={() => { selectWorkspace(intents, project, selected); onUseForNewTask?.(); onBack(); }} type="button"><Plus size={14} />New task here</button> : null}
          <dl>
            <div><dt>Location</dt><dd title={selected.path}>{selected.path}<button aria-label="Copy path" onClick={() => void navigator.clipboard.writeText(selected.path)} type="button"><Copy size={13} /></button>{intents.openFolder && repository ? <button aria-label="Open folder" disabled={selected.availability === "unavailable"} onClick={() => intents.openFolder?.(repository.repositoryId, selected.worktreeId)} type="button"><FolderOpen size={13} /></button> : null}</dd></div>
            <div><dt>Type</dt><dd>{isProjectRoot(selected, project) ? "Project root" : selected.isMain ? "Primary Git worktree" : `${capitalize(selected.ownership)} worktree`}</dd></div>
            <div><dt>Last used</dt><dd>{selected.lastUsedAt ? relativeTime(selected.lastUsedAt) : "Never"}</dd></div>
          </dl>
          <section className="worktree-linked-tasks"><header><strong>Linked tasks · {linked.length}</strong>{linked.length > 8 ? <label><Search size={12} /><input aria-label="Filter linked tasks" onChange={(event) => setLinkedQuery(event.target.value)} placeholder="Filter tasks" value={linkedQuery} /></label> : null}</header><div>
            {visibleLinked.map((task) => <button key={task.task_id} onClick={() => intents.openTask(task.task_id)} type="button"><AgentIcon agentId={task.agent_id} agentName={task.agent_name} size={12} /><span className="title">{task.title}</span><span>{task.status === "active" ? "Running" : relativeTime(task.last_activity)}</span><span>›</span></button>)}
            {!visibleLinked.length ? <p>{linked.length ? "No matching tasks." : "No linked tasks."}</p> : null}
            {tasksError ? <p className="task-workspace-error">{tasksError}</p> : null}
          </div></section>
          {!selected.isMain ? <button className="worktree-remove" onClick={async () => {
            setError(undefined);
            try { setPreflight(await intents.removalPreflight(repository!.repositoryId, selected.worktreeId)); }
            catch (cause) { setError(cause instanceof Error ? cause.message : "Unable to inspect worktree."); }
          }} type="button"><Trash2 size={14} />{selected.availability === "unavailable" ? "Forget worktree…" : "Remove worktree…"}</button> : null}
          {preflight ? <RemovalConfirmation
            linkedTaskCount={selected.linkedTaskCount}
            onCancel={() => setPreflight(undefined)}
            onConfirm={async () => {
              try {
                await intents.removeWorktree(repository!.repositoryId, selected.worktreeId);
                if (selectedTaskWorktreeId === selected.worktreeId && projectRoot) {
                  selectWorkspace(intents, project, projectRoot);
                }
                select(compact ? undefined : projectRoot?.worktreeId);
                setPreflight(undefined);
              } catch (cause) {
                setError(cause instanceof Error ? cause.message : "Unable to remove worktree.");
              }
            }}
            preflight={preflight}
            worktree={selected}
          /> : null}
          {error ? <p className="task-workspace-error">{error}</p> : null}
        </article> : visibleWorktrees.length ? null : <p className="task-workspace-empty">No worktrees found.</p>}
      </div>
    </section>
  );
}

function RemovalConfirmation({ linkedTaskCount, onCancel, onConfirm, preflight, worktree }: {
  linkedTaskCount: number;
  onCancel: () => void;
  onConfirm: () => Promise<void>;
  preflight: WorktreeRemovalPreflight;
  worktree: WorktreeSummary;
}) {
  const forgetting = worktree.availability === "unavailable";
  const taskCopy = linkedTaskCount
    ? `${linkedTaskCount} linked ${linkedTaskCount === 1 ? "Task" : "Tasks"} will remain readable, but cannot continue in this workspace.`
    : "No Task history will be removed.";
  const branchCopy = worktree.head.kind === "branch"
    ? `Branch ${worktree.head.name} will be kept.`
    : "Git references are kept.";
  return <div className="worktree-removal-confirm">
    <strong>{preflight.status === "safe"
      ? forgetting ? `Forget “${worktree.name}”?` : `Remove “${worktree.name}”?`
      : "Worktree cannot be removed"}</strong>
    {preflight.status === "safe" ? <>
      <p>{forgetting
        ? "The folder is already missing. This removes the stale worktree from management."
        : <>The folder <code>{preflight.path}</code> will be deleted and unregistered from Git.</>}</p>
      <p>{taskCopy}</p>
      {!forgetting ? <p>{branchCopy}</p> : null}
    </> : <p>{blockerText(preflight)}</p>}
    <div className="worktree-removal-actions">
      <button onClick={onCancel} type="button">Cancel</button>
      {preflight.status === "safe" ? <button className="danger" onClick={() => void onConfirm()} type="button">{forgetting ? "Forget worktree" : "Remove worktree"}</button> : null}
    </div>
  </div>;
}

function selectWorkspace(intents: NewTaskViewIntents, project: ProjectOption, worktree: WorktreeSummary) {
  const root = isProjectRoot(worktree, project);
  intents.selectWorktree({ worktreeId: root ? undefined : worktree.worktreeId, label: root ? "Project root" : worktree.name, path: worktree.path });
}

function isProjectRoot(worktree: WorktreeSummary, project: ProjectOption) {
  return project.projectWorktreeId
    ? worktree.worktreeId === project.projectWorktreeId
    : worktree.path === project.workspaceRoot;
}

function headLabel(worktree: WorktreeSummary) { return worktree.head.kind === "branch" ? worktree.head.name : `Detached · ${worktree.head.commit.slice(0, 7)}`; }
function chooserMeta(worktree: WorktreeSummary) {
  const activity = worktree.runningTaskCount ? `${worktree.runningTaskCount} task${worktree.runningTaskCount === 1 ? "" : "s"} running` : worktree.linkedTaskCount ? `${worktree.linkedTaskCount} linked task${worktree.linkedTaskCount === 1 ? "" : "s"}` : undefined;
  return [headLabel(worktree), activity, worktree.availability === "unavailable" ? "Unavailable" : undefined].filter(Boolean).join(" · ");
}
function deriveBranchName(name: string, existing: string[] = []) {
  const slug = name.normalize("NFKC").toLocaleLowerCase().replace(/[^\p{L}\p{N}]+/gu, "-").replace(/^-+|-+$/gu, "");
  const segments = typeof Intl.Segmenter === "function"
    ? [...new Intl.Segmenter(undefined, { granularity: "grapheme" }).segment(slug)].map((segment) => segment.segment)
    : Array.from(slug);
  const base = segments.slice(0, 48).join("").replace(/-+$/u, "");
  if (!base || !existing.includes(base)) return base;
  let suffix = 2;
  while (existing.includes(`${base}-${suffix}`)) suffix += 1;
  return `${base}-${suffix}`;
}
function baseLabel(base: NonNullable<WorktreeRepositorySnapshot["bases"]>[number] | undefined) { return base?.kind === "head" ? `${base.label} · ${base.commit.slice(0, 7)}` : "Current HEAD"; }
function capitalize(value: string) { return `${value.slice(0, 1).toLocaleUpperCase()}${value.slice(1)}`; }
function blockerText(preflight: WorktreeRemovalPreflight) { return preflight.blockers.map((blocker) => ({ runningTasks: "A linked task is running.", locked: "Git has locked this worktree.", unavailable: "The folder is unavailable.", workingTreeChanges: "The worktree has uncommitted changes.", detachedCommits: "Detached commits are not preserved by a branch.", initializedSubmodules: "Initialized submodules must be removed first.", primaryWorktree: "The Project root cannot be removed." }[blocker])).join(" "); }

function CheckIcon() { return <span aria-hidden="true">✓</span>; }
