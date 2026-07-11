# Product

## Register

product

## Users

Developers and engineering teams working inside VS Code who want agent assistance without losing editor context. They need to start tasks, follow execution, inspect terminal and tool activity, respond to permission requests, and return to previous task history quickly.

## Product Purpose

OpenAIDE is a VS Code-native agent workbench for managing agent tasks from first prompt through execution history. Success means the user can understand what the agent is doing, recover previous work, and notice runtime problems without leaving the editor.

## Brand Personality

Calm, technical, exact. OpenAIDE should feel like an editor-native work surface: capable, restrained, and trustworthy under repeated daily use.

## Anti-references

- Marketing SaaS dashboards with oversized cards and decorative gradients.
- Chat apps that hide execution details behind message bubbles.
- Modal-heavy workflows for ordinary task actions.
- Colorful AI-themed chrome, glowing effects, playful animation, or large hero moments.
- Interfaces that look separate from VS Code instead of belonging inside it.

## Design Principles

- Best UI/UX is the primary product constraint: architecture, protocol, runtime, and UI choices must preserve immediate feedback, clear progress, recoverable errors, and responsiveness under local Agent or App Server latency.
- Stay native to the editor: use VS Code rhythm, density, commands, focus behavior, and visual tokens.
- Make agent work inspectable: chat messages, folded tool activity, terminal output, permission state, and runtime errors should be visible and attributable when relevant.
- Keep new tasks responsive: opening a new task may start backend preparation, but the UI must render immediately and show preparation, readiness, or setup errors without blocking orientation or local interaction.
- Minimalism is the main design constraint: every visible section must earn its place.
- Prefer quiet controls over spectacle: compact lists, small actions, clear status, minimal decoration.
- Preserve readable history: passive task open should show saved local state without surprising live recovery.

## Accessibility & Inclusion

Aim for WCAG 2.2 AA where applicable inside VS Code webviews. Keyboard navigation, visible focus, semantic busy/disabled states, reduced-motion compatibility, and non-color-only status indicators are required for core task flow.
