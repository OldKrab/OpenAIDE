import { isDeepStrictEqual } from "node:util";
import { randomUUID } from "node:crypto";
import { createReadStream } from "node:fs";
import { readFile, realpath, stat } from "node:fs/promises";
import path from "node:path";

const unsupported = () => new Error(
  "Cannot safely restore this native session's permissions. Select a supported native permission profile in Codex before reopening the session.",
);
const record = (value) => value !== null && typeof value === "object" && !Array.isArray(value);
const onlyKeys = (value, keys) => record(value) && Object.keys(value).every((key) => keys.includes(key));
const failedSessions = new WeakMap();
const recoveryFailure = (code, fresh = false) => Object.assign(new Error(fresh
  ? "Native permission recovery is uncertain. Restart the OpenAIDE App Server, then reopen this session."
  : "Native permission recovery was interrupted. Reopen this session to retry."), { code, fresh });
const writeRecoveryLog = (event) => console.error(JSON.stringify(event));

/**
 * Version-scoped bridge for Codex 0.153.3 native history. Its resume RPC restores
 * model/approval history but rebuilds sandbox permissions from current config.
 * Read the native-owned rollout before resume can append those new defaults;
 * no OpenAIDE option cache or global Codex configuration is replayed or changed.
 */
export async function resumeNativeSession(client, params, { log = writeRecoveryLog } = {}) {
  if (failedSessions.get(client)?.has(params.threadId)) throw recoveryFailure("connection_restart_required", true);
  const { thread } = await client.threadRead({ threadId: params.threadId, includeTurns: false });
  if (thread?.id !== params.threadId || typeof thread.path !== "string" || !path.isAbsolute(thread.path)) {
    throw unsupported();
  }
  const saved = await readNativePolicy(thread.path, params.threadId);
  if (!saved) {
    // Prepared sessions with no accepted turn have no prior policy to recover.
    // The adapter's ordinary initial mode and New Task options still own them.
    return { ...await client.threadResume(params), noPersistedPolicy: true };
  }
  const approvalPolicy = validateApproval(saved.approval_policy);
  // Historical turn_context records predate automatic approval reviewers.
  const approvalsReviewer = saved.approvals_reviewer ?? "user";
  if (!["user", "auto_review", "guardian_subagent"].includes(approvalsReviewer)) throw unsupported();
  const active = saved.active_permission_profile;
  if (active != null && (!onlyKeys(active, ["id", "extends"]) || typeof active.id !== "string")) throw unsupported();
  if (saved.network !== undefined || (active?.id
    && !active.id.startsWith(":") && saved.permission_profile?.network === "enabled")) throw unsupported();
  const { workspacePolicy, ...overrides } = await restorePermissions(saved, params);
  const response = await client.threadResume({ ...params, ...overrides, approvalPolicy, approvalsReviewer });
  if (workspacePolicy) {
    const started = Date.now();
    const fields = { operation: "codex.workspace_policy_restore", session_id: params.threadId, attempt: 1 };
    let failure;
    log({ ...fields, phase: "start" });
    try {
      await applyWorkspaceSettings(client, { threadId: params.threadId, approvalPolicy, approvalsReviewer, sandboxPolicy: workspacePolicy });
    } catch (error) {
      failure = error;
      // The adapter only marks the subscription owned after this helper returns.
      // Unsubscribe releases that ownership; it does not cancel an ambiguous
      // native update. Fence retries on this client until its process is replaced.
      const fence = () => { const ids = failedSessions.get(client) ?? new Set(); ids.add(params.threadId); failedSessions.set(client, ids); };
      if (error.fresh) fence();
      let timer;
      try {
        await Promise.race([client.threadUnsubscribe({ threadId: params.threadId }),
          new Promise((_, reject) => { timer = setTimeout(() => reject(recoveryFailure("cleanup_timeout", true)), 5000); })]);
      } catch { fence(); failure = recoveryFailure("cleanup_failed", true); }
      finally { clearTimeout(timer); }
      throw failure;
    } finally {
      log({ ...fields, phase: "terminal", outcome: failure ? "failure" : "success",
        duration_ms: Date.now() - started, error_code: failure?.code ?? null });
    }
  }
  return { ...response, ...(workspacePolicy && { sandbox: workspacePolicy }), approvalPolicy, approvalsReviewer, usesNativePermissions: true };
}

async function applyWorkspaceSettings(client, params) {
  let timer;
  const observers = [];
  const completed = new Promise((resolve, reject) => {
    // onUnhandledNotification is an additive Event, not an onNotification
    // replacement. The client's existing cache and handlers keep every update.
    observers.push(client.connection.onUnhandledNotification((event) => {
      if (event.method !== "thread/settings/updated" || event.params?.threadId !== params.threadId) return;
      const value = event.params.threadSettings;
      if (isDeepStrictEqual(value?.sandboxPolicy, params.sandboxPolicy)
        && isDeepStrictEqual(value?.approvalPolicy, params.approvalPolicy)
        && value?.approvalsReviewer === params.approvalsReviewer) resolve();
      else reject(recoveryFailure("settings_mismatch", true));
    }));
  });
  const failed = new Promise((_, reject) => {
    timer = setTimeout(() => reject(recoveryFailure("settings_timeout", true)), 15000);
    observers.push(client.connection.onClose(() => reject(recoveryFailure("connection_closed", true))));
    observers.push(client.connection.onDispose(() => reject(recoveryFailure("connection_disposed", true))));
  });
  const update = Promise.resolve().then(() => client.threadSettingsUpdate(params)).catch((error) => {
    // Only protocol validation/method rejection proves non-admission. Local
    // JSON-RPC write/pending errors and native internal errors remain ambiguous.
    const rejected = [-32600, -32601, -32602].includes(error?.code);
    throw recoveryFailure("settings_request_failed", !rejected);
  });
  try { await Promise.race([failed, Promise.all([completed, update])]); }
  finally { clearTimeout(timer); for (const observer of observers) observer.dispose(); }
}

async function readNativePolicy(filename, sessionId) {
  let identity = false;
  let interaction = false;
  let latest;
  try {
    if (!(await stat(filename)).isFile()) throw unsupported();
    const input = createReadStream(filename);
    const decoder = new TextDecoder("utf-8", { fatal: true });
    let parts = [];
    try {
      // Buffer only the current JSONL record, decoding once so malformed UTF-8
      // cannot silently redirect a permission path through replacement text.
      for await (const chunk of input) {
        let start = 0;
        let end;
        while ((end = chunk.indexOf(10, start)) !== -1) {
          parts.push(chunk.subarray(start, end));
          consume(decoder.decode(Buffer.concat(parts)));
          parts = [];
          start = end + 1;
        }
        if (start < chunk.length) parts.push(chunk.subarray(start));
      }
      if (parts.length) consume(decoder.decode(Buffer.concat(parts)));
    } finally { input.destroy(); }
  } catch { throw unsupported(); }
  if (!identity || (interaction && latest === undefined)) throw unsupported();
  return latest;

  function consume(line) {
    if (!line.trim()) return;
    const entry = JSON.parse(line);
    if (!record(entry) || typeof entry.type !== "string" || !record(entry.payload)) throw unsupported();
    if (entry.type === "session_meta") {
      if (entry.payload.id !== sessionId) throw unsupported();
      identity = true;
    } else if (entry.type === "response_item") interaction = true;
    else if (entry.type === "turn_context") latest = entry.payload;
    else if (entry.type === "event_msg" && entry.payload.type === "thread_settings_applied") {
      if (entry.payload.thread_id !== undefined && entry.payload.thread_id !== sessionId) throw unsupported();
      if (!record(entry.payload.thread_settings)) throw unsupported();
      const next = entry.payload.thread_settings;
      const roots = next.workspace_roots ?? (latest?.workspace_roots !== undefined
        ? (next.cwd === latest.cwd ? latest.workspace_roots : null) : undefined);
      latest = { ...next, ...(roots !== undefined && { workspace_roots: roots }),
        ...(latest?.network !== undefined && { network: latest.network }) };
    }
  }
}

function validateApproval(value) {
  if (["untrusted", "on-request", "never"].includes(value)) return value;
  const fields = ["sandbox_approval", "rules", "skill_approval", "request_permissions", "mcp_elicitations"];
  if (!onlyKeys(value, ["granular"]) || !onlyKeys(value.granular, fields)
    || fields.some((key) => typeof value.granular[key] !== "boolean")) throw unsupported();
  return value;
}

function restorePermissions(saved, params) {
  const config = params.config;
  if (saved.permission_profile === undefined) return restoreLegacy(saved, params);
  const profile = saved.permission_profile;
  if (onlyKeys(profile, ["type"]) && profile.type === "disabled") return { sandbox: "danger-full-access" };
  if (!onlyKeys(profile, ["type", "file_system", "network"]) || profile.type !== "managed"
    || !["enabled", "restricted"].includes(profile.network)) throw unsupported();
  const fs = profile.file_system;
  if (!onlyKeys(fs, ["type", "entries", "glob_scan_max_depth"]) || fs.type !== "restricted" || !Array.isArray(fs.entries)) {
    throw unsupported();
  }
  if (fs.entries.some((entry) => record(entry) && entry.missing_path_behavior !== undefined)) {
    return restoreWorkspace(saved, params);
  }
  // A generated named profile would hide historical network proxy constraints
  // and cannot be recovered safely on the next restart. Only legacy workspace
  // construction has a verified enabled-network restoration path here.
  if (profile.network === "enabled") throw unsupported();
  const filesystem = Object.create(null);
  if (fs.glob_scan_max_depth !== undefined) {
    if (!Number.isSafeInteger(fs.glob_scan_max_depth) || fs.glob_scan_max_depth < 1) throw unsupported();
    filesystem.glob_scan_max_depth = fs.glob_scan_max_depth;
  }
  for (const entry of fs.entries) {
    if (!onlyKeys(entry, ["path", "access"]) || !["read", "write", "deny"].includes(entry.access)) throw unsupported();
    const target = entry.path;
    if (onlyKeys(target, ["type", "path"]) && target.type === "path") {
      if (!literalPath(target.path)) throw unsupported();
      addPermission(filesystem, target.path, entry.access);
    } else if (onlyKeys(target, ["type", "value"]) && target.type === "special") {
      const special = target.value;
      if (!onlyKeys(special, ["kind", "subpath"])) throw unsupported();
      if (["root", "minimal", "tmpdir", "slash_tmp"].includes(special.kind) && special.subpath === undefined) {
        addPermission(filesystem, `:${special.kind}`, entry.access);
      } else if (["project_roots", "current_working_directory"].includes(special.kind)) {
        addWorkspacePermission(filesystem, special.subpath ?? ".", entry.access);
      } else throw unsupported();
    } else if (onlyKeys(target, ["type", "pattern"]) && target.type === "glob_pattern" && entry.access === "deny") {
      if (typeof target.pattern !== "string") throw unsupported();
      const prefix = "codex-project-roots://";
      if (target.pattern.startsWith(prefix)) addWorkspacePermission(filesystem, target.pattern.slice(prefix.length), "deny", true);
      else {
        if (!path.isAbsolute(target.pattern) || !/[\*?\[\]]/.test(target.pattern)) throw unsupported();
        addPermission(filesystem, target.pattern, "deny");
      }
    } else throw unsupported();
  }
  const id = `openaide-recovered-${randomUUID()}`;
  if (config !== undefined && (!record(config) || (config.permissions !== undefined && !record(config.permissions)))) throw unsupported();
  if (config?.permissions?.[id] !== undefined || Object.keys(config ?? {}).some((key) => key.startsWith(`permissions.${id}`))) throw unsupported();
  let runtimeWorkspaceRoots = saved.workspace_roots;
  if (runtimeWorkspaceRoots !== undefined || filesystem[":workspace_roots"] !== undefined) {
    if (runtimeWorkspaceRoots === undefined && saved.cwd) runtimeWorkspaceRoots = [saved.cwd];
    if (!Array.isArray(runtimeWorkspaceRoots) || runtimeWorkspaceRoots.some((root) => !literalPath(root))) throw unsupported();
  }
  return {
    permissions: id,
    ...(runtimeWorkspaceRoots && { runtimeWorkspaceRoots }),
    config: { ...config, permissions: { ...config?.permissions, [id]: { filesystem, network: { enabled: profile.network === "enabled" } } } },
  };
}

function restoreLegacy(saved, params) {
  // Older turn contexts use the legacy sandbox enum. A richer unknown split
  // policy must never be replaced by that compatibility projection.
  if (saved.file_system_sandbox_policy !== undefined) throw unsupported();
  const legacy = saved.sandbox_policy;
  if (onlyKeys(legacy, ["type"]) && legacy.type === "danger-full-access") return { sandbox: "danger-full-access" };
  if (!onlyKeys(legacy, ["type", "network_access", "writable_roots", "exclude_tmpdir_env_var", "exclude_slash_tmp"])) throw unsupported();
  for (const key of ["network_access", "exclude_tmpdir_env_var", "exclude_slash_tmp"]) {
    if (legacy[key] !== undefined && typeof legacy[key] !== "boolean") throw unsupported();
  }
  if (legacy.type === "read-only" && onlyKeys(legacy, ["type", "network_access"])) {
    return restorePermissions({ ...saved, permission_profile: {
      type: "managed", network: legacy.network_access ? "enabled" : "restricted",
      file_system: { type: "restricted", entries: [
        { path: { type: "special", value: { kind: "root" } }, access: "read" },
      ] },
    } }, params);
  }
  const roots = legacy.writable_roots ?? [];
  if (legacy.type !== "workspace-write" || !Array.isArray(roots) || roots.some((root) => !literalPath(root))) throw unsupported();
  return workspaceOverrides(params.config, roots, legacy.network_access ?? false,
    legacy.exclude_tmpdir_env_var ?? false, legacy.exclude_slash_tmp ?? false);
}

async function restoreWorkspace(saved, params) {
  const fs = saved.permission_profile.file_system;
  const cwd = saved.cwd;
  if (!literalPath(cwd) || (params.cwd !== undefined && params.cwd !== cwd) || fs.glob_scan_max_depth !== undefined) throw unsupported();
  const writes = new Set();
  const protectedPaths = new Set();
  const temporary = new Set();
  let rootRead = false;
  for (const entry of fs.entries) {
    if (!onlyKeys(entry, ["path", "access", "missing_path_behavior"])) throw unsupported();
    const target = entry.path;
    if (onlyKeys(target, ["type", "path"]) && target.type === "path" && literalPath(target.path)) {
      if (entry.access === "write" && entry.missing_path_behavior === undefined) writes.add(target.path);
      else if (entry.access === "read" && entry.missing_path_behavior === "skip") protectedPaths.add(target.path);
      else throw unsupported();
    } else if (onlyKeys(target, ["type", "value"]) && target.type === "special"
      && onlyKeys(target.value, ["kind"]) && entry.missing_path_behavior === undefined) {
      if (target.value.kind === "root" && entry.access === "read") rootRead = true;
      else if (["tmpdir", "slash_tmp"].includes(target.value.kind) && entry.access === "write") temporary.add(target.value.kind);
      else throw unsupported();
    } else throw unsupported();
  }
  const expectedProtected = new Set([...writes].flatMap((root) => [".git", ".agents", ".codex"].map((name) => path.join(root, name))));
  const gitDirectory = await worktreeGitDirectory(cwd);
  if (gitDirectory) expectedProtected.add(gitDirectory);
  if (!rootRead || !writes.has(cwd) || protectedPaths.size !== expectedProtected.size
    || [...expectedProtected].some((entry) => !protectedPaths.has(entry))) throw unsupported();
  // Native legacy workspace construction owns these protected read/skip rules.
  // Named-profile grammar cannot encode skip; only this exact canonical shape
  // may use the legacy constructor. Any custom carveout fails before resume.
  return workspaceOverrides(params.config, [...writes].filter((root) => root !== cwd),
    saved.permission_profile.network === "enabled", !temporary.has("tmpdir"), !temporary.has("slash_tmp"));
}

async function worktreeGitDirectory(cwd) {
  const gitFile = path.join(cwd, ".git");
  let metadata;
  try { metadata = await stat(gitFile); }
  catch (error) { if (error.code === "ENOENT") return undefined; throw unsupported(); }
  try {
    if (metadata.isDirectory()) return undefined;
    if (!metadata.isFile() || metadata.size > 65536) throw unsupported();
    const match = /^gitdir: ([^\r\n]+)\r?\n?$/.exec(await readFile(gitFile, "utf8"));
    if (!match) throw unsupported();
    const directory = await realpath(path.resolve(cwd, match[1]));
    const backlink = (await readFile(path.join(directory, "gitdir"), "utf8")).trim();
    const common = await realpath(path.resolve(directory, (await readFile(path.join(directory, "commondir"), "utf8")).trim()));
    if (await realpath(path.resolve(directory, backlink)) !== await realpath(gitFile)
      || path.dirname(directory) !== path.join(common, "worktrees")) throw unsupported();
    return directory;
  } catch { throw unsupported(); }
}

function workspaceOverrides(original, roots, network, excludeTmpdir, excludeSlashTmp) {
  const config = Object.fromEntries(Object.entries(original ?? {}).filter(([key]) => !key.startsWith("sandbox_workspace_write.")));
  return {
    sandbox: "read-only",
    workspacePolicy: { type: "workspaceWrite", writableRoots: roots, networkAccess: network,
      excludeTmpdirEnvVar: excludeTmpdir, excludeSlashTmp: excludeSlashTmp },
    config: { ...config, sandbox_workspace_write: {
      writable_roots: roots, network_access: network,
      exclude_tmpdir_env_var: excludeTmpdir, exclude_slash_tmp: excludeSlashTmp,
    } },
  };
}

function literalPath(value) {
  return typeof value === "string" && path.isAbsolute(value) && !/[\*?\[\]\0]/.test(value);
}

function addWorkspacePermission(filesystem, subpath, access, glob = false) {
  if (typeof subpath !== "string" || path.isAbsolute(subpath) || subpath.includes("\0")
    || (subpath !== "." && subpath.split(/[\\/]/).some((part) => part === ".." || part === "." || part === ""))
    || (!glob && /[\*?\[\]]/.test(subpath))) throw unsupported();
  filesystem[":workspace_roots"] ??= Object.create(null);
  addPermission(filesystem[":workspace_roots"], subpath, access);
}

function addPermission(entries, key, access) {
  // Native equal-specificity conflict precedence is deny > write > read.
  const rank = { read: 0, write: 1, deny: 2 };
  if (entries[key] === undefined || rank[access] > rank[entries[key]]) entries[key] = access;
}

// TODO(codex-acp-1.2.0): remove with an upstream release that restores native
// model and policy on resume/load. AgentMode presets are shared singletons;
// the resumed policy always belongs to this session and must never mutate them.
export function recoveredSessionMode(native, AgentMode) {
  if (native === undefined || native.noPersistedPolicy) return AgentMode.getInitialAgentMode();
  if (native.approvalPolicy === undefined || native.approvalsReviewer === undefined || !native.sandbox) {
    throw new Error("Native session did not return its approval policy.");
  }
  const policy = structuredClone({
    approvalPolicy: native.approvalPolicy,
    approvalsReviewer: native.approvalsReviewer,
    sandboxPolicy: native.sandbox,
  });
  const preset = !native.usesNativePermissions && AgentMode.all().find((candidate) => (
    isDeepStrictEqual(candidate.approvalPolicy, policy.approvalPolicy)
    && isDeepStrictEqual(candidate.approvalsReviewer, policy.approvalsReviewer)
    && isDeepStrictEqual(candidate.sandboxPolicy, policy.sandboxPolicy)
  ));
  const mode = new AgentMode(
    preset?.id ?? "native",
    preset?.name ?? "Native session policy",
    preset?.description ?? "Preserves this session's existing approval and sandbox policy",
    preset?.kind ?? "standard",
    policy.approvalPolicy,
    policy.approvalsReviewer,
    policy.sandboxPolicy,
    preset?.sandboxMode,
  );
  // The resume-only profile declaration cannot be reselected by turn/start.
  // Omit permission overrides until an explicit preset change replaces mode.
  mode.usesNativePermissions = native.usesNativePermissions === true;
  if (preset) return mode;

  // An arbitrary native policy cannot be rounded to a preset: doing so could
  // grant network/file access or change who approves the next operation.
  mode.toSessionModeState = () => ({
    availableModes: [...AgentMode.all().map((value) => value.toSessionMode()), mode.toSessionMode()],
    currentModeId: mode.id,
  });
  mode.toConfigOption = () => {
    const option = AgentMode.getInitialAgentMode().toConfigOption();
    return {
      ...option,
      currentValue: mode.id,
      options: [...option.options, {
        value: mode.id, name: mode.name, description: mode.description, _meta: { kind: mode.kind },
      }],
    };
  };
  return mode;
}
