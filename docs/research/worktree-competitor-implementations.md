# Worktree lifecycle in AI coding products

Research date: 2026-07-15

## Question and terminology

This note investigates one specific lifecycle question for OpenAIDE: can a product expose a useful new-task composer before creating a Git worktree, then materialize the worktree lazily without starting an agent against an empty directory?

The products use the word **session** for different things, so this report distinguishes:

- **Draft**: composer UI and selections that exist before the first prompt is accepted.
- **Product task/thread record**: the host application's own persisted identity. This is not necessarily an agent-native session.
- **Agent session**: the Codex/Claude/ACP/SDK conversation bound to a working directory.
- **First turn**: dispatch of the user's first prompt to the agent or model.
- **Materialization**: successful `git worktree add`/checkout, including the tracked files, not merely reserving an empty path.

## Conclusion

The strongest finding is negative: **none of the investigated products is evidenced to start a generic agent session in an empty placeholder directory and populate that same directory later**.

Instead, current implementations choose one of three lifecycle shapes:

1. **Draft first; materialize on Send; then start the agent.** T3 Code and the Codex desktop app do this. VS Code and the GitHub Copilot app also expose isolation, model, and agent choices before Send, although their public documentation does not specify the exact internal order between worktree creation and SDK-session allocation after Send.
2. **Materialize as part of explicit session launch.** Claude Code's `--worktree` command creates the checkout and then starts Claude in it. Claude Desktop automatically gives each new Code session a worktree.
3. **Materialize when the user selects the worktree, before the first prompt.** Zed deliberately moved to this design in April 2026. Its previous implementation deferred creation until first-prompt Send, but Zed removed that machinery to make creation eager.

This matters for OpenAIDE. The lazy products can offer pre-Send controls because they own those controls or defer the underlying agent harness; they do not prove that arbitrary ACP Agent options and slash commands can be discovered without `session/new`. Zed is the closest ACP comparison, and it chose an already-materialized checkout rather than a placeholder.

The proposed OpenAIDE placeholder lifecycle therefore remains novel and agent-sensitive:

```text
reserve empty final path
-> ACP session/new(empty path)
-> discover Agent options and slash commands
-> first Send materializes the checkout in place
-> session/prompt on the existing ACP session
```

No primary source found here demonstrates that an arbitrary agent reliably re-reads repository context after that in-place transition. The competitors support a safer invariant: **when an agent session is created or begins work, its working directory is already a real checkout**.

## Comparison

| Product | Pre-Send draft/configuration | When checkout is materialized | When agent begins | Placeholder/lazy checkout | Branch and location | Cleanup/failure behavior |
| --- | --- | --- | --- | --- | --- | --- |
| **T3 Code** | Yes. The first message bootstraps the thread; the composer shows “Preparing worktree” while bootstrap runs. | On first Send, before setup and before turn dispatch. | After worktree creation and setup. | Lazy until Send, but no agent is started in an empty directory. | Temporary `t3code/<8 hex>` branch; `<worktreesDir>/<repoName>/<branch-with-slashes-replaced>`. | On deletion of the last thread linked to a worktree, UI offers to force-remove the worktree. A removal failure leaves the thread deleted and reports a toast. Partial-creation cleanup/adoption remain unknown. |
| **Zed** | Yes; threads may have drafts, and the title bar owns worktree selection. | Current behavior: eagerly when the user chooses “create worktree,” not on first Send. | Public docs do not expose the exact ACP `session/new` ordering. The agent is scoped to the selected real worktree. | No placeholder documented. Zed explicitly removed its prior first-Send-lazy creation machinery. | Detached `HEAD`; current or default branch as base; default `../worktrees/<project>/`, configurable. | Archive saves Git state and removes a managed worktree when unused; restore recreates it; delete removes conversation and associated worktree data. Release notes record cleanup of partial files/Git metadata after creation failure. |
| **Codex desktop app** | Yes. Select Worktree, base branch, and optional environment under the composer. | After prompt submission. | Documentation says submission creates the worktree and Codex works there; no ACP layer is involved. | Lazy until Send, but the product creates the checkout for the task rather than starting Codex in an empty placeholder. | Detached `HEAD` under `$CODEX_HOME/worktrees`; optional local changes can be applied; permanent worktrees are also supported. | Managed worktrees are archived/snapshotted and restored; recent-worktree limit defaults to 15; active, pinned, and permanent worktrees are protected from automatic deletion. |
| **Claude Code CLI** | No graphical task draft; launch arguments choose isolation. | `claude --worktree` creates the worktree as part of launch. An in-session `EnterWorktree` tool can also create/switch later. | The command creates the worktree and starts Claude in it. | No empty placeholder. Sparse checkout and symlink settings reduce large-repository cost. | `.claude/worktrees/<name>/`, branch `worktree-<name>`; default base is `origin/HEAD`, with fallback/configuration for local `HEAD`. | Clean unnamed worktrees can be removed automatically at exit; changed worktrees prompt to keep/remove; noninteractive worktrees are retained. Named sessions prompt before cleanup. |
| **Claude Desktop** | A new Code session is the user-visible unit. | A worktree is created automatically for every new Git-backed Code session. | Exact internal ordering is not documented, but each session operates in its own worktree. | No placeholder documented. | `.claude/worktrees/` by default, configurable, with configurable branch prefix. | Archiving removes the worktree; optional auto-archive follows PR merge/close. |
| **VS Code Copilot CLI session** | Yes. Worktree/Folder isolation, custom agent, model, and permissions are selected before submitting the prompt; slash commands are available in Copilot CLI chat. | The docs say VS Code creates a separate worktree for a worktree-isolated session, but do not say whether directory creation occurs at isolation selection or only after Send. | The agent starts working after prompt submission. The precise worktree-create versus SDK-session-create ordering is not public. | No placeholder documented. | Separate folder; exact path, base ref, and branch naming are not documented on the inspected page. | Forked sessions share the original worktree. Archive removes a clean checkout but preserves branch/commits for restoration; deletion removes associated worktrees after the last linked session is gone. |
| **GitHub Copilot app** | Yes. Repository, new worktree/local/cloud location, mode, model, effort, prompt references, and slash commands are chosen before the agent starts. | Not specified precisely in public docs. | The agent starts after the user completes those selections and submits the prompt. | No placeholder documented. Quick chats intentionally avoid a dedicated branch/worktree. | Each full session has an isolated workspace/branch; exact local path and branch algorithm were not established. | Not established from the inspected session-start documentation. |

## T3 Code

T3 Code is the clearest implementation of “lazy until first Send” without an empty-directory agent session.

At pinned commit `ecb35f75839925dd1ac6f854efeef5c9e291d11b`, the first-send path in [`ChatView.tsx`](https://github.com/pingdotgg/t3code/blob/ecb35f75839925dd1ac6f854efeef5c9e291d11b/apps/web/src/components/ChatView.tsx) creates a temporary branch name and requests bootstrap; [`ChatComposer.tsx`](https://github.com/pingdotgg/t3code/blob/ecb35f75839925dd1ac6f854efeef5c9e291d11b/apps/web/src/components/chat/ChatComposer.tsx) presents the worktree-preparation state. The temporary branch convention is `t3code/<8 hex characters>` in [`packages/shared/src/git.ts`](https://github.com/pingdotgg/t3code/blob/ecb35f75839925dd1ac6f854efeef5c9e291d11b/packages/shared/src/git.ts).

The server bootstrap in [`apps/server/src/ws.ts`](https://github.com/pingdotgg/t3code/blob/ecb35f75839925dd1ac6f854efeef5c9e291d11b/apps/server/src/ws.ts) performs the important steps in this order:

```text
create T3 thread record
-> resolve/fetch base ref
-> create worktree
-> update thread metadata and Git status
-> run worktree setup
-> dispatch finalTurnStartCommand
```

Thus the T3 thread record can precede checkout creation, but the coding-agent turn does not. The worktree driver sets `targetBranch` to the new branch when one is requested, otherwise to the selected ref, and sanitizes `/` to `-` for the directory name. Its default path is `<worktreesDir>/<repoName>/<sanitizedBranch>`. For a new branch it executes the equivalent of `git worktree add -b <newRefName> <path> <refName>`; otherwise it uses `git worktree add <path> <refName>` ([`GitVcsDriverCore.ts`](https://github.com/pingdotgg/t3code/blob/ecb35f75839925dd1ac6f854efeef5c9e291d11b/apps/server/src/vcs/GitVcsDriverCore.ts)). The root is defined by [`config.ts`](https://github.com/pingdotgg/t3code/blob/ecb35f75839925dd1ac6f854efeef5c9e291d11b/apps/server/src/config.ts).

This architecture is available to T3 because its product thread and its provider turn are distinct. It does not need to start Codex, Claude, or OpenCode merely to render the draft.

When the user deletes the last thread linked to a worktree, the UI identifies the orphan and asks whether to delete the worktree too. Confirmation invokes forced removal using the origin project's workspace root and the linked worktree path. If removal fails, thread deletion is not rolled back; the UI reports the cleanup failure ([`ChatView.tsx`](https://github.com/pingdotgg/t3code/blob/ecb35f75839925dd1ac6f854efeef5c9e291d11b/apps/web/src/components/ChatView.tsx), [`GitVcsDriverCore.ts`](https://github.com/pingdotgg/t3code/blob/ecb35f75839925dd1ac6f854efeef5c9e291d11b/apps/server/src/vcs/GitVcsDriverCore.ts)).

**Unknown:** the pinned sources did not establish whether worktree preparation can adopt an existing directory, whether failed creation removes partial filesystem and Git metadata, or whether removing the worktree also removes its branch. The “failure prevents dispatch” statement is a code-level inference from the sequential bootstrap rather than a documented product guarantee.

## Zed

Zed is the most relevant comparison because its external agents use ACP. A thread may use either the built-in Zed Agent or an ACP External Agent, and the user can choose a worktree from the title bar ([Parallel Agents](https://zed.dev/docs/ai/parallel-agents)).

Current Zed behavior is eager. Selecting “create worktree” materializes a linked worktree immediately, puts it in detached `HEAD`, and switches the workspace to it. The Git documentation says the user may base it on the current or default branch, name it or accept an automatic name, and configure its directory with `git.worktree_directory`; the default resolves to `../worktrees/<project>/` ([Git worktrees](https://zed.dev/docs/git#git-worktrees), [worktree-directory setting](https://zed.dev/docs/configuring-zed#git-worktree-directory)). A `create_worktree` task hook runs after materialization with both the new and main checkout paths ([Task hooks](https://zed.dev/docs/tasks#hooks)).

The lifecycle is not accidental. In April 2026, Zed merged PR [#53941](https://github.com/zed-industries/zed/pull/53941), whose summary says it:

- makes creation eager as soon as the user chooses the current/default base;
- removes the `StartThreadIn` machinery that previously delayed creation until first-prompt Send;
- shows explicit creating/loading states;
- does not auto-submit the draft prompt when switching worktrees.

That historical implementation proves Zed once supported a lazy first-Send boundary, but neither the PR nor current docs say that an ACP session had been started against an empty reserved directory. It is not evidence for the OpenAIDE placeholder design.

For lifecycle management, archiving an inactive thread saves its Git state and removes the managed worktree when no other active thread uses it; restoring the thread restores the worktree; permanent deletion removes conversation and worktree data ([Parallel Agents](https://zed.dev/docs/ai/parallel-agents#worktree-isolation)). Zed's v0.233.0 release notes also record fixes for cleaning orphaned files and Git metadata when new-thread worktree creation fails and for incomplete cleanup on archive ([Zed v0.233.0 release notes](https://github.com/zed-industries/zed/releases/tag/v0.233.0-pre)).

**Unknown:** public sources do not expose the exact moment Zed calls ACP `session/new` relative to worktree picker activation, workspace switching, and draft-thread creation. They establish only that the current selected workspace is a materialized worktree and that External Agents are ACP-backed.

## Codex desktop app

Codex offers a true pre-Send draft. In the new-task view the user selects **Worktree**, a base branch, and optionally a local environment. On prompt submission, Codex creates the checkout and begins the task there ([Codex worktree setup](https://learn.chatgpt.com/docs/environments/git-worktrees.md#worktree-setup)).

Managed worktrees live under `$CODEX_HOME/worktrees` by default, use the selected branch's `HEAD` as the starting commit, and remain detached until the user explicitly creates a branch. If the selected source has local changes, Codex can apply them to the worktree. `.worktreeinclude` copies selected ignored files without copying every untracked file ([Codex worktree management](https://learn.chatgpt.com/docs/environments/git-worktrees.md#how-codex-manages-worktrees-for-you)).

The task keeps its associated worktree across Local/Worktree handoffs. Codex snapshots before automatic deletion, offers restoration, keeps the 15 most recent managed worktrees by default, and exempts pinned, active, and permanent worktrees from automatic deletion ([Codex cleanup](https://learn.chatgpt.com/docs/environments/git-worktrees.md#worktree-cleanup)).

This is strong evidence for a product-owned draft followed by materialization on Send. It is not evidence for early ACP or Codex App Server session creation; the public product documentation does not describe such a session boundary.

## Claude Code

Claude Code's CLI is worktree-first. `claude --worktree <name>` creates `.claude/worktrees/<name>/` on a new `worktree-<name>` branch and starts Claude in that checkout; an omitted name is generated. The default base is the remote default branch (`origin/HEAD`), with fallback to local `HEAD`, and `worktree.baseRef` can choose the local-HEAD behavior explicitly ([Claude Code worktrees](https://code.claude.com/docs/en/worktrees)).

Claude also exposes two cost controls that are relevant for large repositories: `worktree.sparsePaths` checks out only selected monorepo paths, while `worktree.symlinkDirectories` shares selected large directories such as dependency caches ([Claude Code worktree settings](https://code.claude.com/docs/en/settings#worktree-settings)). These optimize a real checkout; they do not create a semantically empty placeholder.

At exit, Claude automatically removes clean unnamed worktrees, prompts before removing named or changed worktrees, and leaves noninteractive `-p` worktrees for manual cleanup. The in-session `EnterWorktree` tool is a different lifecycle: it lets an already-running Claude conversation create or enter a worktree and later exit it ([worktree cleanup](https://code.claude.com/docs/en/worktrees#clean-up-worktrees), [tool reference](https://code.claude.com/docs/en/tools-reference)). That agent-driven switch is supported by Claude itself and is not a portable assumption for arbitrary ACP agents.

Claude Desktop says every new Git-backed Code session automatically receives its own worktree under `.claude/worktrees/`, configurable in settings, and that archiving the session removes the worktree ([Claude Desktop sessions](https://code.claude.com/docs/en/desktop#work-in-parallel-with-sessions)). The docs do not expose a lower-level session/worktree ordering.

## VS Code and GitHub Copilot

VS Code's Copilot CLI session composer lets the user select Worktree or Folder isolation before starting. It also exposes custom-agent, model, and permission configuration, and Copilot CLI chat supports slash commands. The agent starts working after the user submits the prompt ([Copilot CLI sessions](https://code.visualstudio.com/docs/agents/agent-types/copilot-cli), [Agents window startup](https://code.visualstudio.com/docs/agents/agents-window#start-an-agent-session)).

Worktree isolation creates a separate folder and makes it the Copilot SDK session's workspace. A conversation fork shares its parent's worktree. Archiving removes a clean worktree but preserves its branch and commits so restoration can recreate the checkout; deletion removes the worktree only after the final linked session is archived or deleted ([VS Code chat-session lifecycle](https://code.visualstudio.com/docs/chat/chat-sessions#archive-sessions)).

The GitHub Copilot app exposes an even clearer session draft: after clicking **+**, the user chooses a repository, **new working tree/local repository/cloud sandbox**, mode, model, effort, and then writes a prompt with `#`, `@`, and `/` references. Only after submission does the agent start. Quick chats explicitly provide saved conversation without a dedicated branch/worktree ([GitHub Copilot app sessions](https://docs.github.com/en/copilot/how-tos/github-copilot-app/agent-sessions#starting-a-session)).

**Unknown:** neither public document specifies whether the local worktree directory is materialized immediately when isolation is selected or during post-Send startup, nor the exact ordering against Copilot SDK session allocation. They therefore demonstrate pre-Send host-owned configuration, not an agent session safely observing an initially empty directory.

## Implications for OpenAIDE

1. **Do not treat a host task record as ACP `session/new`.** T3's thread-before-worktree sequence works precisely because provider dispatch comes later. Codex and Copilot similarly own the composer and its options.
2. **The closest ACP precedent points toward eager materialization.** Zed supports ACP External Agents and intentionally moved away from first-Send-lazy creation.
3. **Placeholder materialization is an agent compatibility contract, not a Git trick.** Git can create the checkout in a reserved empty directory, but ACP does not promise that an already-created session will rediscover the repository, instructions, capabilities, or project-specific state before its first prompt.
4. **There is another viable product choice besides the placeholder:** make choosing Worktree an explicit asynchronous preparation action. Show the composer while creation runs, but do not call `session/new` until the checkout is ready. This matches current Zed and Claude CLI. It delays ACP-provided options and slash commands, which is the UX cost already identified.
5. **A sessionless draft becomes viable only if OpenAIDE owns enough pre-Send metadata.** Static/cached controls could imitate T3, Codex, or Copilot, but live ACP Agent options and slash commands remain unavailable until `session/new`; caching them would introduce staleness and agent-specific semantics.
6. **If OpenAIDE adopts the placeholder anyway, qualify and test agents explicitly.** The minimum compatibility test is: `session/new` in the reserved empty final path, materialize a representative repository in place, then send the first prompt and verify that the agent observes files, repository root, instructions, slash commands, and configuration exactly as it would from a session created after checkout. No competitor evidence lets OpenAIDE assume this generically.

## Source scope and confidence

Only primary sources were used: official product documentation, official release notes, and first-party source code/PRs at stable commits. No claim relies on a third-party comparison article.

Confidence is high for T3's first-Send ordering, Zed's eager-current/lazy-former behavior, Codex's submit-then-create flow, and Claude CLI's create-then-launch flow. Confidence is deliberately bounded for internal native-session allocation in Zed, VS Code, GitHub Copilot, Codex Desktop, and Claude Desktop because their public sources describe user-visible behavior rather than internal protocol calls.
