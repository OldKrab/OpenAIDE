# Mobile task-context selector patterns

Research date: 2026-07-18

## Question

How should OpenAIDE represent three pre-task selections—**Project**, **Task Workspace**, and **Agent**—on a 360–430 px mobile screen without horizontal scrolling, accidental wrapping, or an opaque combined summary?

This note evaluates current first-party product patterns and the constraints already exposed by the OpenAIDE implementation. It does not change the accepted Task Workspace design or production code.

## Conclusion

There is another viable solution besides squeezing, truncating, or wrapping all three selectors: **stop presenting all three as peer controls on mobile**.

This conclusion crosses an existing product decision. OpenAIDE's accepted specification currently places Project, Task Workspace, and Agent together in the start-context row, and the user previously rejected moving Agent into the composer. The research therefore identifies a credible alternative, not an already-approved change. If Agent must remain outside the composer, there is no layout-only technique that preserves three unbounded full-text values in one narrow row; one value must become icon-only, contextual, or truncated.

The strongest direction for OpenAIDE is:

```text
[ OpenAIDE ▾ ] [ shushakov/cancell… ▾ ]

┌ Describe the task…                    ┐
│                                      │
│ +   Codex ▾          More · 4 ▾   ↑  │
└──────────────────────────────────────┘
```

- Keep **Project + Task Workspace** together immediately above the composer as the execution-location context.
- Move the visible **Agent** selector into the composer footer on mobile.
- Let the existing adaptive composer packing move lower-priority Agent-owned run options under **More** when space is tight.
- Keep the approved Task Workspace chooser and its mobile sheet unchanged.
- Keep the current three-control row on desktop, where it fits.

This is not hiding context or adding a setup step. All three current values remain visible and one tap away, directly around the prompt. It is a semantic split: Project and Task Workspace answer **where**, while Agent answers **who interprets this prompt**.

The closest first-party precedent is Cursor for iOS. Its published new-agent composer shows repository and branch together at the top of the composer (`acme main`) and the selected model in the composer footer (`Composer 2.5`). Cursor does not attempt to fit repository, branch, and model as three peer chips above the prompt. [Cursor Mobile](https://cursor.com/mobile), [Cursor iOS App Store listing, screenshot 2](https://apps.apple.com/us/app/cursor/id6767085653)

Zed independently places its model selector on the message editor rather than in a separate project-context toolbar. [Zed Agent Panel](https://zed.dev/docs/ai/agent-panel#changing-models)

GitHub Mobile follows the same progressive-disclosure principle. Repository is selected from the prompt field; base branch, custom Agent, and model are optional. GitHub's multi-Agent flow selects the Agent through an icon in the input field rather than a third permanent text label. GitHub also permits creation from a Repository view, letting entry context supply the repository. [GitHub Mobile cloud Agent](https://docs.github.com/en/copilot/how-tos/use-copilot-agents/cloud-agent/use-cloud-agent-on-mobile), [GitHub Agent picker](https://github.blog/changelog/2026-02-04-claude-and-codex-are-now-available-in-public-preview-on-github/), [GitHub Mobile task entry](https://github.blog/changelog/2025-09-24-start-and-track-copilot-coding-agent-tasks-in-github-mobile/)

## Why the current problem is structural

The names are not bounded:

- Project labels can be long and localized.
- Worktree display names and Git refs are routinely long.
- Agent labels can be custom.
- Every independent selector also pays for an icon, chevron, padding, spacing, and a touch target.

Therefore, no three-text-control layout can guarantee useful visible content at 360 px. CSS can guarantee that it does not overflow, but only by making one or more values unreadably short. This is why the one-row experiment technically fit short labels and failed with a real branch, while the two-row experiment created a visually orphaned Workspace control.

The accessibility floor also limits how aggressively controls can be compressed. WCAG 2.2 requires targets to contain at least a 24 × 24 CSS-pixel area or satisfy its spacing exception; closely packed undersized controls are specifically called out as a failure mode. [W3C Understanding SC 2.5.8](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum)

Primer also warns that truncating an interactive element is not a complete accessibility strategy: the full value must remain available through an explicit interaction, not a hover-only title. [Primer Truncate accessibility guidance](https://primer.style/product/components/truncate/accessibility/)

## Evidence from adjacent products

### Cursor mobile: location at the top, model at the bottom

Cursor's first-party iOS screenshot is the closest direct analogue:

```text
acme main ▾
[ prompt text ]
+   Composer 2.5 ▾               send
```

The observation is literal—the official page and App Store screenshot expose those labels in those positions. The inference for OpenAIDE is that Agent belongs with prompt controls on mobile, while Project and Workspace form the location context. Cursor's July 2026 picker redesign also describes repository, run location, and branch as a drill-in hierarchy rather than one flat set of permanent controls. [Cursor Mobile](https://cursor.com/mobile), [Cursor picker redesign](https://cursor.com/changelog)

### GitHub Mobile: required context first, optional choices compact

GitHub's official mobile flow requires a repository, then treats base branch, custom Agent, and model as optional selections. Its multi-Agent announcement says the Agent is selected through the Copilot icon in the input field. A separate GitHub Mobile launch flow starts directly from Home or a Repository view. The literal observation is that GitHub does not give repository, branch, Agent, and model equal persistent text width; the inference is that OpenAIDE should similarly prioritize location and compact or relocate Agent on narrow screens. [GitHub Mobile cloud Agent](https://docs.github.com/en/copilot/how-tos/use-copilot-agents/cloud-agent/use-cloud-agent-on-mobile), [GitHub Agent picker](https://github.blog/changelog/2026-02-04-claude-and-codex-are-now-available-in-public-preview-on-github/), [GitHub Mobile task entry](https://github.blog/changelog/2025-09-24-start-and-track-copilot-coding-agent-tasks-in-github-mobile/)

### Zed: model selection belongs to the editor

Zed's official Agent Panel documentation says the model is changed through the model selector **on the message editor**. This supports placing a prompt-interpreter choice in the composer, although Zed's model is not identical to OpenAIDE's ACP Agent. [Zed Agent Panel](https://zed.dev/docs/ai/agent-panel#changing-models)

### Codex cloud: prompt context shows repository and branch

OpenAI's original Codex cloud task UI is described as a prompt box with repository and branch selectors. Codex itself is fixed in that product, so it does not solve a three-selector case, but it reinforces that repository/branch is one location layer around the task prompt. [Introducing Codex](https://openai.com/index/introducing-codex/)

### GitHub Codespaces: defaults and hierarchy instead of permanent parallel controls

GitHub's standard Codespaces flow starts from a repository and selected branch, then creates with defaults. Less common choices move to **New with options**, where Branch, Dev container configuration, Region, and Machine type become explicit fields. This is evidence for progressive disclosure and context-first selection, not evidence that OpenAIDE should copy the separate page. [Creating a codespace](https://docs.github.com/en/codespaces/developing-in-a-codespace/creating-a-codespace-for-a-repository)

### VS Code and Primer: one trigger may open a rich mobile picker

VS Code recommends multi-step Quick Picks for related-but-separate basic selections. Primer's SelectPanel supports filtering, grouping, and secondary actions, and its narrow-screen variant becomes a full-screen selection surface. These are sound patterns for the contents of OpenAIDE's Project or Task Workspace chooser, but they do not require replacing the visible current value with one generic “Task setup” button. [VS Code Quick Picks](https://code.visualstudio.com/api/ux-guidelines/quick-picks#multiple-steps), [Primer SelectPanel](https://www.primer.style/product/components/select-panel/), [Primer narrow-screen behavior](https://primer.style/product/getting-started/react/migration-guides/primer-experimental-selectpanel#narrow-screens-mobile)

## OpenAIDE implementation evidence

The recommended direction fits existing seams rather than inventing a new composite control:

- [`NewTaskView.tsx`](../../packages/frontend/src/components/NewTaskView.tsx) already renders Project, Task Workspace, and Agent as separate selectors and explicitly suppresses the composer's Agent selector with `showAgentSelector={false}`.
- [`ComposerMenus.tsx`](../../packages/frontend/src/components/ComposerMenus.tsx) already supports rendering the selected Agent as a normal composer control.
- [`ComposerRunOptions.tsx`](../../packages/frontend/src/components/ComposerRunOptions.tsx) already packs Agent-owned run controls into **More** when they no longer fit. This gives a mobile Agent control a stable slot without forcing every run option to remain visible.
- [`NewTaskView.tsx`](../../packages/frontend/src/components/NewTaskView.tsx) already supports `fixedProjectContext`, omitting the Project selector when the task was opened in a known Project context.
- The rejected two-row result is currently encoded in [`mobile-chat.css`](../../packages/frontend/src/styles/app/mobile-chat.css): Project and Agent occupy the first grid row and Workspace spans the second. The visual failure is not a data-model limitation.

One product-language caveat needs explicit treatment: the composer may already expose an Agent-owned option whose selected value is named **Agent**. If the ACP Agent selector moves into the footer, its concrete name and brand icon (`Codex`) must remain visible, while lower-priority Agent-owned controls should pack into **More**. Showing `Codex` beside another generic `Agent` control would be confusing.

## Option evaluation

### A. Move Agent into the composer footer — strongest

```text
[ Project ▾ ] [ Task Workspace ▾ ]
[ prompt                                  ]
[ + ] [ Codex ▾ ]             [ More ▾ ][↑]
```

**Why it works**

- Removes an entire peer control from the constrained location row without hiding any selected value.
- Leaves roughly twice as much variable text width for Workspace.
- Keeps Agent at the user's focus, closer to the prompt than it is today.
- Matches Cursor mobile's location/model split and Zed's editor-level model selector.
- Reuses OpenAIDE's existing Composer Agent selector and adaptive option packing.
- Does not change Task acquisition, Agent preparation, slash-command discovery, or Worktree behavior.

**Tradeoffs**

- Mobile and desktop place Agent differently; the visual adaptation must feel intentional.
- Agent-owned controls may need to pack under **More** earlier.
- The two concepts named “Agent” must not appear next to each other without clarification.

**Fit:** strongest general mobile solution.

### B. Agent icon-only; retain Project + Workspace text — viable but conditional

```text
[ OpenAIDE ▾ ] [ shushakov/cancell… ▾ ] [ ◉ ]
```

**Why it works**

- Saves the Agent label and one chevron's width while keeping the existing control order.
- Codex and OpenCode have recognizable brand icons in OpenAIDE.
- Minimal implementation and no movement between desktop and mobile.

**Why it is weaker**

- OpenAIDE supports custom Agents with a shared library of generic icons; an icon is not a unique Agent identity.
- The current Agent becomes invisible to users who do not recognize its mark.
- Starting in the wrong Agent is materially more consequential than hiding a secondary toolbar action.
- It helps, but Workspace still needs truncation for real branch names.

**Fit:** acceptable only when exactly one Agent is enabled, or as a user-chosen compact mode. It should not be the default multi-Agent representation.

### C. Hierarchical Task location showing only the leaf + Agent — viable only in fixed Project context

```text
[ Sidebar scrolling ▾ ]                  [ Codex ▾ ]
```

The trigger shows only the selected Worktree/Project-root leaf. Opening it shows Projects containing their Worktrees.

**Why it works**

- Two controls fit comfortably.
- Gives Workspace the dominant width it needs.
- A grouped searchable picker can still change Project and Workspace.

**Why it is weaker in global New Task**

- `Project root` is not unique or informative without its Project.
- Worktree display names and branch names can repeat across Projects.
- The user already rejected relying on the distant screen header for Project context.
- A trigger that silently changes two domain selections is harder to understand than it looks.

**Fit:** strong when `fixedProjectContext` is true; weak as the sole global New Task control.

### D. Project-contextual New Task entry — structurally clean, but a workflow change

```text
OpenAIDE
  + New task here

New Task:
[ Project root ▾ ] [ Codex ▾ ]
```

Start New Task from a Project group, Worktree manager, or Task preview. Project is fixed and therefore omitted.

**Why it works**

- Removes Project from the task form rather than abbreviating it.
- Matches OpenAIDE's Project-grouped task navigation.
- Existing `fixedProjectContext` support makes the underlying view feasible.

**Why it is not a complete replacement**

- Changes the meaning and discoverability of the global **New task** action.
- Requires a deliberate fallback for global creation and project switching.
- Adds navigation decisions before the user can type if used as the only entry.

**Fit:** excellent supplemental entry and potentially a later default after user testing; not the smallest fix for the current global flow.

### E. Dynamic flex + truncation for all three — technically valid, visually fragile

```text
[ OpenAI… ▾ ] [ shushakov/canc… ▾ ] [ Codex ▾ ]
```

Give Project and Agent bounded content widths, give Workspace the remainder, and ellipsize every label correctly.

**Why it works**

- Preserves the accepted desktop ordering and one-row interaction.
- Lowest behavioral change.
- Can guarantee no collision or page overflow.

**Why it remains weak**

- Guarantees layout containment, not comprehension.
- Three real values cannot all receive useful text width at 360 px.
- Localization and custom labels make fixed allocations brittle.
- The full value is only discoverable after opening the control; hover disclosure does not exist on mobile.
- The user has already seen the characteristic result: technically one line, visually crushed.

**Fit:** fallback if preserving identical desktop/mobile placement is more important than visible context. It is not the strongest UX.

## Patterns considered and rejected for this screen

- **Horizontal scrolling:** hides state off-screen and turns ordinary setup into navigation.
- **Accidental or intentional two-row peer controls:** creates an orphaned Workspace row and too much vertical rhythm before the composer.
- **One “Task setup” summary trigger:** compact but conceals three independently important current choices and was already rejected by the user.
- **Move Project into the distant page header:** technically frees space but separates the choice from the prompt focus; already rejected by the user.
- **Icon-only controls for all three:** does not communicate custom Project, Workspace, or Agent identities.
- **A mandatory multi-step wizard:** VS Code supports the mechanics, but it adds friction to every New Task and conflicts with OpenAIDE's preference against modal-heavy ordinary workflows.

## Recommended mobile prototype

Prototype only the semantic split first:

1. Restore one compact location row directly above the composer.
2. Render Project as content-sized but capped; render Workspace as `minmax(0, 1fr)` with ellipsis and a full accessible name.
3. Render the concrete Agent selector in the composer's stable control area after **Add context**.
4. Let Agent-owned run controls use the existing **More** packing behavior.
5. Keep every visible touch target at least 30–36 px high, comfortably above WCAG's 24 px minimum.
6. Test at 360, 390, and 430 px with:
   - a 24-character Project label;
   - a 48-character Unicode Worktree name;
   - Codex, OpenCode, and a long custom Agent label;
   - browser text zoom at 200%;
   - one and multiple enabled Agents.
7. Verify that changing Agent or Task Workspace preserves the prompt and still exposes authoritative Agent options and slash commands before Send.

If that prototype is rejected visually, the next experiment should be the **project-contextual New Task entry**, not another attempt to compress three peer text controls.

## Confidence and boundaries

The observed product facts are sourced from official product pages, official documentation, official App Store assets, and OpenAIDE's current source. The recommendation is an inference from those facts and OpenAIDE's constraints.

Confidence is high that three unbounded text selectors cannot be made robustly readable at 360 px without hiding, truncating, wrapping, scrolling, or relocating one dimension. Confidence is high that moving Agent into the composer is technically aligned with existing OpenAIDE components. Visual acceptance still needs a real Target prototype because no external product has OpenAIDE's exact combination of ACP Agent selection and Agent-owned runtime options.
