import { Archive } from "lucide-react";
import type { AgentListedSession } from "@openaide/app-shell-contracts";
import type {
  TaskNavigationSearchHit,
  TaskNavigationSearchResult,
} from "@openaide/app-server-client";
import { chatReveal } from "../state/chatReveal";
import { AgentIcon, type AgentIconLookup } from "./AgentIcon";
import { relativeTime } from "./taskSurfaceHelpers";
import { useNavigationSearch, type RunNavigationSearch } from "./useNavigationSearch";

type SidebarSearchResultsProps = {
  activeTaskId?: string;
  agentIcons?: AgentIconLookup;
  /** Shells that list only OpenAIDE Tasks leave Native Sessions out of the results. */
  includeNativeSessions: boolean;
  onOpenNativeSession: (session: AgentListedSession) => void;
  onOpenTask: (taskId: string) => void;
  query: string;
  search: RunNavigationSearch;
};

/**
 * The sidebar list while the search field has text (ADR-0086, "Search"). It shows App
 * Server's one-shot answer, not the Navigation view, which stays subscribed beneath it.
 */
export function SidebarSearchResults({
  activeTaskId,
  agentIcons,
  includeNativeSessions,
  onOpenNativeSession,
  onOpenTask,
  query,
  search,
}: SidebarSearchResultsProps) {
  const { state, retry } = useNavigationSearch(query, search);
  const shown: TaskNavigationSearchResult | undefined = state.status === "ready"
    ? state.result
    : state.status === "loading" ? state.previous : undefined;
  const hits = shown?.hits.filter((hit) => includeNativeSessions || hit.row.kind === "task") ?? [];
  return (
    <div className="task-list-shell">
      <div aria-busy={state.status === "loading"} aria-label="Search results" className="task-list search-results" role="list">
        {hits.map((hit) => (
          <SearchHitRow
            activeTaskId={activeTaskId}
            agentIcons={agentIcons}
            hit={hit}
            key={hitKey(hit)}
            onOpenNativeSession={onOpenNativeSession}
            onOpenTask={onOpenTask}
          />
        ))}
        {state.status === "error" ? (
          <p className="empty-list search-status" role="alert">
            Search failed.{" "}
            <button className="search-retry" onClick={retry} type="button">Try again</button>
          </p>
        ) : null}
        {state.status === "loading" && hits.length === 0 ? (
          <p aria-live="polite" className="empty-list search-status">Searching</p>
        ) : null}
        {state.status === "ready" && hits.length === 0 ? (
          <p aria-live="polite" className="empty-list search-status">No results</p>
        ) : null}
        {shown?.truncated && hits.length > 0 ? (
          <p className="search-status search-truncated">Showing the best matches. Refine the search to narrow them.</p>
        ) : null}
      </div>
    </div>
  );
}

function SearchHitRow({
  activeTaskId,
  agentIcons,
  hit,
  onOpenNativeSession,
  onOpenTask,
}: {
  activeTaskId?: string;
  agentIcons?: AgentIconLookup;
  hit: TaskNavigationSearchHit;
  onOpenNativeSession: (session: AgentListedSession) => void;
  onOpenTask: (taskId: string) => void;
}) {
  const row = hit.row;
  const agentId = row.kind === "task" ? row.task.agentId : row.session.reference.agentId;
  const rawTitle = row.kind === "task" ? row.task.title?.value : row.session.title ?? undefined;
  const title = rawTitle?.trim() ? rawTitle : "Untitled task";
  // Highlights index the title App Server matched; the placeholder has none.
  const titleRanges = rawTitle?.trim() ? hit.titleHighlights ?? [] : [];
  const activity = row.kind === "task" ? row.task.lastActivity : row.session.lastActivity ?? undefined;
  const age = activity ? relativeTime(activity) : "";
  const selected = row.kind === "task" && row.task.taskId === activeTaskId;
  const open = () => {
    if (row.kind === "task") {
      if (hit.messageId) chatReveal.request(row.task.taskId, hit.messageId);
      onOpenTask(row.task.taskId);
      return;
    }
    onOpenNativeSession(nativeSessionOf(row.session));
  };
  return (
    <div role="listitem">
      <button
        aria-current={selected ? "true" : undefined}
        className={`search-hit ${selected ? "selected" : ""}`}
        onClick={open}
        type="button"
      >
        <span aria-hidden="true" className="task-agent-icon">
          <AgentIcon agentId={agentId} agentName={agentId} icon={agentIcons?.[agentId]} size={12} />
        </span>
        <span className="search-hit-body">
          <span className="search-hit-title">
            <span className="task-title"><Highlighted ranges={titleRanges} text={title} /></span>
            {hit.section === "archive" ? (
              <span aria-label="Archived" className="search-hit-section" role="img" title="In the Archive">
                <Archive aria-hidden="true" size={11} />
              </span>
            ) : null}
            {age ? <span className="task-meta-age">{age}</span> : null}
          </span>
          {hit.snippet ? (
            <span className="search-hit-snippet">
              <Highlighted ranges={hit.snippet.highlights} text={hit.snippet.text} />
            </span>
          ) : null}
        </span>
      </button>
    </div>
  );
}

/** Marks `[start, end)` ranges counted in UTF-16 code units, which `String.slice` also uses. */
export function Highlighted({ ranges, text }: { ranges: ReadonlyArray<readonly [number, number]>; text: string }) {
  const parts: Array<{ marked: boolean; text: string }> = [];
  let cursor = 0;
  for (const [start, end] of ranges) {
    if (start < cursor || end <= start || end > text.length) continue;
    if (start > cursor) parts.push({ marked: false, text: text.slice(cursor, start) });
    parts.push({ marked: true, text: text.slice(start, end) });
    cursor = end;
  }
  if (cursor < text.length) parts.push({ marked: false, text: text.slice(cursor) });
  return (
    <>
      {parts.map((part, index) => part.marked
        ? <mark key={index}>{part.text}</mark>
        : <span key={index}>{part.text}</span>)}
    </>
  );
}

function hitKey(hit: TaskNavigationSearchHit) {
  return hit.row.kind === "task"
    ? `task:${hit.row.task.taskId}`
    : `session:${hit.row.session.reference.agentId}:${hit.row.session.reference.sessionId}`;
}

// TODO(state-sync phase 7 merge): share the Navigation phase's Native Session mapper
// (`mapProtocolTaskNavigation`) once it lands, instead of this local conversion.
function nativeSessionOf(
  session: Extract<TaskNavigationSearchHit["row"], { kind: "nativeSession" }>["session"],
): AgentListedSession {
  return {
    agent_id: session.reference.agentId,
    agent_name: session.reference.agentId,
    session_id: session.reference.sessionId,
    project_id: session.projectId,
    cwd: session.workspaceRoot,
    title: session.title ?? undefined,
    pinned: session.pinned,
    last_activity: session.lastActivity ?? undefined,
  };
}
