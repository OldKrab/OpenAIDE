---
name: OpenAIDE
description: Modern cross-platform desktop workspace for inspectable agent work.
colors:
  app-bg: "light-dark(oklch(0.975 0.006 255), oklch(0.19 0.008 255))"
  sidebar-bg: "light-dark(oklch(0.955 0.008 255), oklch(0.22 0.009 255))"
  surface: "light-dark(oklch(0.99 0.004 255), oklch(0.245 0.009 255))"
  raised: "light-dark(oklch(0.935 0.009 255), oklch(0.285 0.01 255))"
  selected: "light-dark(oklch(0.905 0.018 255), oklch(0.33 0.018 255))"
  text-primary: "light-dark(oklch(0.24 0.012 255), oklch(0.91 0.007 255))"
  text-secondary: "light-dark(oklch(0.48 0.012 255), oklch(0.7 0.009 255))"
  border-strong: "light-dark(oklch(0.72 0.012 255), oklch(0.43 0.012 255))"
  border-soft: "color-mix(in oklch, {colors.border-strong} 52%, transparent)"
  border-subtle: "color-mix(in oklch, {colors.border-strong} 28%, transparent)"
  accent: "light-dark(oklch(0.56 0.18 255), oklch(0.72 0.14 255))"
  warning: "light-dark(oklch(0.58 0.13 75), oklch(0.78 0.12 78))"
  danger: "light-dark(oklch(0.56 0.19 28), oklch(0.74 0.15 26))"
  success: "light-dark(oklch(0.52 0.14 145), oklch(0.74 0.12 145))"
typography:
  screen-title:
    fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI Variable', 'Segoe UI', system-ui, sans-serif"
    fontSize: "20px"
    fontWeight: 650
    lineHeight: 1.25
    letterSpacing: "-0.01em"
  title:
    fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI Variable', 'Segoe UI', system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 600
    lineHeight: 1.4
    letterSpacing: "normal"
  body:
    fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI Variable', 'Segoe UI', system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.5
    letterSpacing: "normal"
  chat:
    fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI Variable', 'Segoe UI', system-ui, sans-serif"
    fontSize: "15px"
    fontWeight: 400
    lineHeight: 1.55
    letterSpacing: "normal"
  label:
    fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI Variable', 'Segoe UI', system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 550
    lineHeight: 1.3
    letterSpacing: "normal"
  mono:
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace"
    fontSize: "12.5px"
    fontWeight: 400
    lineHeight: 1.5
    letterSpacing: "normal"
rounded:
  sm: "6px"
  md: "10px"
  lg: "14px"
spacing:
  xxs: "2px"
  xs: "4px"
  sm: "8px"
  md: "12px"
  lg: "16px"
  xl: "24px"
  xxl: "32px"
components:
  button-primary:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.surface}"
    rounded: "{rounded.sm}"
    padding: "7px 12px"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.sm}"
    padding: "6px 8px"
  list-row:
    backgroundColor: "transparent"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.sm}"
    height: "34px"
  composer:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.lg}"
    padding: "14px 16px"
---

# Design System: OpenAIDE

<!-- SEED -->

## 1. Overview

**Creative North Star: "The Focused Desktop Workspace"**

OpenAIDE should feel like a polished application designed specifically for sustained agent work on macOS and Windows. It combines the clarity and keyboard fluency of excellent developer tools with the spatial confidence, finish, and platform integration expected from a modern desktop product.

The desktop application defines the visual standard. Web and VS Code shells share the product language and behavior, then adapt platform-specific window chrome, menus, focus rules, tokens, and file access. VS Code is a supported environment, not the aesthetic template.

**Key Characteristics:**

- Clear visual hierarchy with purposeful whitespace.
- Project and task navigation optimized for repeated daily use.
- Controls whose shape and emphasis reflect their role.
- Agent chat with folded, inspectable activity rather than raw logs.
- Native shell integration for folders, menus, shortcuts, notifications, and windows.
- Refined light and dark themes with consistent semantic roles.
- Motion that explains state changes and otherwise stays out of the way.

## 2. Visual Direction

OpenAIDE is restrained, but restraint does not mean grey, tiny, flat, or anonymous. The interface should have recognizable proportions, confident typography, carefully tuned surfaces, and precise interaction states.

Use whitespace to separate concepts, not to imitate a marketing page. Use density where users compare or scan many items, such as Task Navigation. Use more breathing room where users compose, read, decide, or recover from an error.

### Quality rules

- Start with the relationship that needs to be communicated, then choose alignment, typography, spacing, containers, borders, and surfaces that make it clear.
- Choose persistent or transient control surfaces according to discoverability, context, input method, and visual hierarchy.
- Choose equal or content-driven sizing according to the meaning of the options, the realistic value range, and the surrounding composition.
- Balance breathing room with information density so space supports comprehension rather than following a fixed aesthetic preference.
- Prefer a coherent icon language, while allowing intentional variation when a platform or concept benefits from it.
- Judge optical alignment from screenshots. Mathematical centering is not sufficient when icons, labels, and chevrons have different visual weight.

## 3. Color And Themes

OpenAIDE supports first-class light, dark, and system themes. Theme selection belongs in Settings and follows the operating-system preference when set to System.

The palette uses lightly tinted neutrals rather than pure black, pure white, or cold default grey. Accent color is reserved for focus, selection, progress, and the primary safe action. Status colors always pair with text or iconography.

Desktop implementation uses OpenAIDE semantic tokens as its shared vocabulary. A host shell may map or extend those roles when platform conventions materially improve the experience.

### Surface roles

- **App background:** the primary reading and working plane.
- **Sidebar background:** a distinct navigation plane with enough contrast to establish structure without a heavy divider.
- **Surface:** composers, popovers, menus, inputs, and grouped interaction.
- **Raised:** hover, active, transient, and selected-adjacent states.
- **Selected:** current project, task, option, or navigation destination.

### Named rules

**The Accent Rarity Rule.** Accent is a state signal, not decoration. Large accent-filled regions require a specific product reason.

**The Theme Parity Rule.** Light and dark themes must preserve hierarchy and state clarity. Neither theme is a recolored afterthought.

**The Border Purpose Rule.** Borders communicate a real boundary, input affordance, focus, or selection. They are not the default method for making every item visible.

## 4. Typography

The system UI stack provides native text rendering on macOS and Windows. One sans family carries the interface. Monospace is reserved for paths, commands, revisions, terminal output, and other code-like values.

### Hierarchy

- **Screen title** (20px, 650): primary page question or destination title.
- **Title** (14px, 600): task titles, popover headings, and meaningful group names.
- **Body** (14px, 400): controls, settings, descriptions, and ordinary interface copy.
- **Chat** (15px, 400): user and agent prose.
- **Label** (12px, 550): compact metadata and short structural labels.
- **Mono** (12.5px, 400): paths, commands, branches, and technical identifiers.

Use weight, size, color, and spacing together. Muted text can support hierarchy, but important selected values and actions remain comfortably readable.

## 5. Spacing And Layout

Use the spacing scale as a starting vocabulary, then tune optical relationships. Repeating the same gap everywhere produces a mechanical interface.

- Related icon and label: `4px` to `6px`.
- Controls inside a compact group: `4px` to `8px`.
- Label to supporting description: `2px` to `4px`.
- Related sections: `12px` to `16px`.
- Major page regions: `24px` to `32px`.

Desktop layouts should make deliberate use of the window. Navigation may remain dense, while the main work surface should have a readable maximum width and stable alignment. Narrow windows change structure at explicit breakpoints rather than squeezing desktop controls until they fail.

Choose centering when it supports focus or balance, such as empty and start states. Choose scan-oriented alignment when users compare or read active work. Other compositions are valid when they better serve the task.

## 6. Elevation And Shape

Use tonal layers first. Transient UI such as menus, popovers, and floating inspection surfaces may use a restrained shadow plus a soft boundary. Persistent surfaces should not float without reason.

Corner radius follows scale and role:

- Small controls and list selections: `6px`.
- Menus, inputs, and compact grouped surfaces: `10px`.
- Composer and larger focused surfaces: `14px`.

Shared radius, fill, and border can create useful coherence. Vary them when role, scale, hierarchy, or interaction calls for a different silhouette.

## 7. Components

### Buttons

- Primary buttons use accent fill for the main safe action.
- Secondary buttons use a visible but quieter surface when they must remain discoverable.
- Ghost buttons are transparent at rest and gain a surface on hover, focus, or open state.
- Icon buttons need a clear accessible name and a hit target appropriate to the shell.
- Button width follows the composition: content-driven, equal, fluid, or fixed sizing are all available when their behavior is deliberate.

### Inline selectors

- Inline selectors should read as editable values within their surrounding sentence or context.
- Use a semantic icon, selected value, and chevron where they improve recognition.
- Choose permanent or transient surfaces based on how much affordance the selector needs in its current context.
- Selected values use intentional emphasis. Frequency alone does not determine whether an option should appear quieter.
- Long values truncate only when the full value is available in the opened menu and, where useful, nearby context.

### Inputs

- Inputs use a clear surface and focus treatment because typing requires a stable boundary.
- Labels and errors stay near the input, with tooltip support used only as an additional explanation.
- Provide a persistent label when the field's purpose would otherwise become ambiguous.

### Popovers And Menus

- Menus align to their trigger and remain within the current window.
- Use one concise heading only when the choices need role or consequence explained.
- Rows have stable icon, label, metadata, selection, and action positions.
- Technical consequences, such as Isolation behavior, belong inside the relevant popup rather than bloating the main composition.
- Use restrained elevation so transient surfaces are clearly above the page without appearing detached.

### Navigation

- Task Navigation uses scan-friendly rows, stable action placement, and clear selected, unread, failed, waiting, and active states.
- Choose navigation density from scanning needs, pointer and keyboard behavior, window size, and realistic content.
- Project groups clarify ownership; cards, indentation, separators, or other structures may be used when they communicate that hierarchy well.
- Archive access stays near search or filtering rather than becoming a permanent secondary section.
- Runtime health, support export, and telemetry details do not live in Task Navigation.
- Narrow windows truncate metadata before actions and avoid unstable wrapped row controls.

### Composer

- The composer anchors the active task flow and aligns with the readable chat column.
- The input grows until it would crowd history, then scrolls internally.
- Place context actions where their effect is understandable. A consolidated toolbar is appropriate when it improves discovery and remains coherent at realistic widths.
- Send becomes stop or cancel during active agent work.

### Chat And Activity

- Agent chat prioritizes readable conversation with compact folded activity.
- Sequential tool calls group under one meaningful natural-language summary.
- Individual calls usually render as supporting details, but richer cards, logs, or terminal treatments are appropriate when the underlying content needs them.
- Long user content and noisy tool output collapse with controls outside clipped content.

## 8. Platform Adaptation

### macOS

- Respect native menu placement, keyboard conventions, traffic-light safe areas, fullscreen behavior, and system file dialogs.
- Use macOS typography and focus behavior through the system stack rather than drawing a fake macOS theme.

### Windows

- Respect native window controls, keyboard access, snap layouts, scaling, notifications, and system file dialogs.
- Verify common display scaling levels and adapt intentionally across device-pixel ratios.

### Web

- Provide deliberate browser-safe substitutes for operating-system file and folder access.
- Preserve product structure and visual quality while designing explicitly for browser capabilities and limitations.

### VS Code

- Integrate commands, focus, file reveal, and host capabilities where useful.
- Map semantic roles to host tokens when useful for legibility and integration. Adopt VS Code density or component styling where it genuinely improves that shell, without assuming it should transfer elsewhere.

## 9. Motion

Most interaction transitions use `120ms` to `180ms` ease-out timing. Other timing and motion models are appropriate when they clarify spatial change, direct attention, express product character, or support a platform convention. Prefer compositor-friendly animation; use layout animation when its explanatory value justifies the cost.

Respect reduced-motion preferences across every shell.

## 10. Design Review Checklist

Before presenting a UI direction:

- Review the complete composition, not an isolated component crop.
- Capture realistic desktop and narrow-window screenshots in both light and dark themes when color or elevation changed.
- Exercise default, hover, focus, open, selected, loading, empty, error, disabled, and long-value states that affect the decision.
- Check overflow and display scaling.
- Ask whether each border, fill, icon, label, and container has a specific job.
- Ask whether each control's treatment is the strongest fit for its role, including when a simple rectangle is the clearest answer.
- Evaluate asymmetry, empty space, hierarchy, and density in the complete composition rather than treating any of them as inherently good or bad.
- Treat peer critique as evidence, not approval. The final judgment comes from the rendered composition and the product intent.

## 11. Contextual Evaluation

No component, visual effect, layout family, or aesthetic is prohibited by name. Cards, modals, gradients, glass effects, dense layouts, sparse layouts, equal-width controls, asymmetric compositions, editor-inspired patterns, decorative moments, and unconventional interactions can all be excellent choices in the right context.

For any significant design choice, ask:

- What user problem or product quality does this choice improve?
- Why is it a better fit here than the credible alternatives?
- Does it remain effective with realistic content, states, themes, platforms, and window sizes?
- Is its familiarity useful, or is it being copied without the original context?
- Does consistency help the user transfer knowledge, or would meaningful distinction improve comprehension?
- Does the visual expression support the work, product character, or moment being designed?
- Is the execution polished enough for the ambition of the idea?

Use these questions to challenge defaults and weak reasoning, not to narrow the available design vocabulary.
