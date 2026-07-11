---
name: OpenAIDE
description: VS Code-native agent workbench for inspectable task execution.
colors:
  editor-bg: "#1f1f1f"
  panel-bg: "#252526"
  raised-bg: "#2a2d2e"
  selected-bg: "#37373d"
  text-primary: "#cccccc"
  text-secondary: "#9da1a6"
  border-soft: "#3c3c3c"
  accent: "#4d9cff"
  warning: "#d7ba7d"
  danger: "#f48771"
  success: "#89d185"
typography:
  title:
    fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 600
    lineHeight: 1.35
    letterSpacing: "normal"
  body:
    fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.45
    letterSpacing: "normal"
  label:
    fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif"
    fontSize: "11px"
    fontWeight: 500
    lineHeight: 1.25
    letterSpacing: "normal"
  mono:
    fontFamily: "var(--vscode-editor-font-family), ui-monospace, SFMono-Regular, Menlo, Consolas, monospace"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.45
    letterSpacing: "normal"
rounded:
  sm: "4px"
  md: "6px"
  lg: "8px"
spacing:
  xs: "4px"
  sm: "8px"
  md: "12px"
  lg: "16px"
  xl: "24px"
components:
  button-primary:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.editor-bg}"
    rounded: "{rounded.sm}"
    padding: "6px 10px"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.sm}"
    padding: "4px 8px"
  list-row:
    backgroundColor: "transparent"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.sm}"
    height: "28px"
  composer:
    backgroundColor: "{colors.panel-bg}"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.lg}"
    padding: "10px 12px"
---

# Design System: OpenAIDE

<!-- SEED -->

## 1. Overview

**Creative North Star: "The Editor Workbench"**

OpenAIDE should feel like a native technical workbench inside VS Code: dense, quiet, inspectable, and built for repeated use. The interface should borrow the editor's rhythm rather than introduce a separate product personality.

The system rejects marketing SaaS dashboards, oversized chat bubbles, raw log styling, decorative AI chrome, modal-heavy ordinary workflows, and hero-like presentation. Structure, focus, and state clarity carry the design.

**Key Characteristics:**
- Compact list-first navigation.
- Editor-token surfaces with restrained contrast.
- Agent chat with folded activity, not logs.
- Inline recovery and permission states.
- No persistent runtime/status/support export section in Task Navigation.
- Minimal motion tied to state changes only.

## 2. Colors

The palette is restrained and host-native: VS Code tokens are canonical in implementation, while these seed values describe fallback roles for contexts where host tokens are unavailable.

### Primary
- **Workbench Accent** (`#4d9cff`): current selection, focus reinforcement, and primary send-style actions. Use sparingly.

### Neutral
- **Editor Ground** (`#1f1f1f`): base editor-like background.
- **Panel Ground** (`#252526`): sidebar, composer, popover, and settings surfaces.
- **Raised Ground** (`#2a2d2e`): hover, selected-adjacent surfaces, and active popover rows.
- **Selected Ground** (`#37373d`): selected task rows and active navigation states.
- **Primary Text** (`#cccccc`): labels, authored content, and readable prose.
- **Secondary Text** (`#9da1a6`): metadata, timestamps, descriptions, and low-priority counters.
- **Soft Border** (`#3c3c3c`): quiet separation between dense areas.

### Status
- **Warning** (`#d7ba7d`): permission requests, blocked states, and caution copy.
- **Danger** (`#f48771`): failed runtime state, denied actions, and destructive warnings.
- **Success** (`#89d185`): completed, applied, or healthy states.

### Named Rules

**The Accent Rarity Rule.** The accent is signal, not decoration; keep it under 10% of any screen.

**The Host Token Rule.** Prefer VS Code tokens in implementation. Fallback values exist only to keep standalone development legible.

## 3. Typography

**Display Font:** none
**Body Font:** system UI stack
**Label/Mono Font:** VS Code editor monospace for code-like content

**Character:** Native, compact, and legible. One sans family carries the product UI; monospace appears only where editor semantics require it.

### Hierarchy
- **Title** (600, 13px, 1.35): task row titles, panel headings, popover active labels.
- **Body** (400, 13px, 1.45): chat prose, composer text, settings descriptions.
- **Label** (500, 11px, 1.25): section headers, metadata labels, compact counters.
- **Mono** (400, 12px, 1.45): paths, commands, terminal output, counters, diff-like content.

### Named Rules

**The No Display Type Rule.** Product surfaces do not use display fonts, fluid headings, or hero-scale typography.

## 4. Elevation

OpenAIDE uses tonal layering and borders rather than decorative shadows. Surfaces are flat at rest. Depth appears through token layers, one-pixel borders, hover fills, and focus outlines.

### Named Rules

**The Flat By Default Rule.** Shadows are not a primary depth mechanism. Use them only if a host-native popover requires separation that borders cannot provide.

## 5. Components

### Buttons
- **Shape:** compact rectangle with small radius (`4px`).
- **Primary:** accent fill, compact padding (`6px 10px`), used for the main safe action only.
- **Hover / Focus:** hover shifts tonal layer; focus uses visible host focus border.
- **Ghost:** transparent at rest, tonal hover, used for secondary actions and icon buttons.

### Chips
- **Style:** quiet raised or bordered surface, compact text, optional chevron when mutable.
- **State:** editable before task start; read-only after locked. Locked must look intentional, not disabled by accident.

### Cards / Containers
- **Corner Style:** medium radius (`6px` to `8px`) only for composer, popovers, permission blocks, and grouped chat activity.
- **Background:** host panel or raised surface.
- **Shadow Strategy:** none by default.
- **Border:** soft one-pixel border when separation is needed.
- **Internal Padding:** `10px` to `14px` for dense panels.

### Inputs / Fields
- **Style:** host input background, one-pixel border, compact padding.
- **Focus:** host focus border with no layout shift.
- **Error / Disabled:** inline text near the field; no tooltip-only dependency.

### Navigation
- **Style:** one default task list with dense rows (`24px` to `32px`), leading state mark, title, metadata, and stable action slot.
- **States:** default, hover, selected, focused, failed, inactive, and external states must be distinguishable by more than color alone.
- **Archive:** archived tasks are reached through a small in-place filter/control near search, not a separate persistent sidebar section.
- **Mobile / narrow panel:** truncate metadata before actions; do not wrap row controls into unstable columns.
- **Runtime:** runtime health, support export, and telemetry details do not live in Task Navigation. First iteration support export is a hidden command.

### Composer
- **Style:** docked raised surface aligned to the chat column.
- **Behavior:** multiline input grows until it would crowd chat, then scrolls internally.
- **Actions:** send becomes stop/cancel during active agent work.

### Chat
- **Style:** conversational agent chat with compact folded activity, readable agent prose, distinct authored input, and bounded terminal output.
- **Behavior:** long user content and noisy tool output collapse with controls outside clipped content.
- **Tool activity:** sequential tool calls are grouped under one meaningful natural-language title describing the work performed. Individual calls render as low-emphasis details with muted icons, short labels, and optional tiny metadata. They should feel like Codex/Claude Code activity lines, not cards, log rows, or terminal blocks.

## 6. Do's and Don'ts

### Do:
- **Do** use VS Code tokens for background, foreground, selection, focus, warning, danger, success, and borders.
- **Do** keep sidebar rows between `24px` and `32px` high.
- **Do** keep the sidebar minimal: task search, new task, default task list, archive access.
- **Do** align composer and chat to the same readable column.
- **Do** show status through icon or text as well as color.
- **Do** keep tool-call summaries visually quieter than user/agent prose.
- **Do** keep empty states small and immediately actionable.

### Don't:
- **Don't** build marketing SaaS dashboards with oversized cards and decorative gradients.
- **Don't** make chat look like logs or raw execution traces.
- **Don't** turn routine tool calls into bulky cards.
- **Don't** use modal-heavy workflows for ordinary task actions.
- **Don't** add colorful AI-themed chrome, glowing effects, playful animation, or large hero moments.
- **Don't** make the interface look separate from VS Code.
- **Don't** add runtime health, support export, or telemetry sections to Task Navigation.
- **Don't** use side-stripe borders, gradient text, glassmorphism, nested cards, or decorative motion.
