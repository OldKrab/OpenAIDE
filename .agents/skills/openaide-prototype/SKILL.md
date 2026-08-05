---
name: openaide-prototype
description: Build production-aligned OpenAIDE UI or logic prototypes that answer a concrete design question. Use when the user asks to prototype, compare variants, explore a component or page, validate an interaction, test a state model, or approve a direction before production implementation. Reuse real product components and context, switch UI variants without page reloads, and preserve the approved appearance and behavior during production hardening.
---

# OpenAIDE Prototype

Build a focused experiment that lets the user make one product or engineering decision quickly. Treat the approved result as a UX or behavior contract, not as disposable insight and not as automatically production-ready code.

Follow [`docs/prototyping.md`](../../../docs/prototyping.md) for repository paths, commands, Target access, verification, and cleanup.

## Choose the prototype shape

- For appearance, hierarchy, or interaction questions, build a UI prototype in the existing prototype harness.
- For state transitions, data shape, or algorithms that need manual exploration, build a logic prototype.
- If the question is ambiguous, ask which decision the user needs to make. If the surrounding work makes the answer clear, state the question and proceed.

## Set the decision boundary

Before coding, write one sentence naming the question the prototype must answer. Identify what approval will lock:

- UI approval locks visible structure, styling, content hierarchy, and interaction behavior demonstrated by the candidate.
- Logic approval locks observable rules, transitions, and outcomes demonstrated by the candidate.

Internal implementation may change during production integration. Do not change the approved experience or semantics without showing the difference and obtaining agreement.

## Stay production-aligned, not production-complete

Build candidates quickly enough that several can be compared:

- Reuse real production components, design tokens, styles, types, and representative data shapes.
- Import surrounding production UI directly. Do not recreate headers, navigation, controls, or other existing context inside the prototype.
- Add a small prototype-local adapter or fixture when real product state is too expensive or unsafe to connect.
- Keep external mutations stubbed and persistence in memory unless integration or persistence is the question.
- Include basic accessibility, realistic content density, responsive behavior needed for review, and enough error or empty-state behavior to evaluate the decision.
- Skip exhaustive tests, defensive handling, persistence wiring, and abstractions that do not affect the decision.
- Add a focused test or real integration only when the prototype question cannot be answered reliably without it.

After approval, harden only the selected direction with production tests, lifecycle handling, persistence, logging, and error recovery required by its real boundary.

## Build a UI prototype

1. Read the relevant product component, its surrounding page or shell, `PRODUCT.md`, and `DESIGN.md` before drafting variants.
2. Create the prototype with `npm run prototype:new -- <prototype-name>`.
3. Prefer three structurally different variants; use one for a narrow question and never exceed five. Variants must differ in hierarchy, layout, or interaction—not only color or wording.
4. Import real production components into `packages/frontend/prototypes/<prototype-name>/prototype.tsx`. Keep only candidate-specific composition and fixture adapters local to the prototype.
5. Use the standard harness `?variant=<key>` control. Variant changes must update the URL and React state in place; do not navigate or reload the page.
6. Keep reviewer-shareable scenario state in query parameters when reload or sharing must reproduce it.
7. Run `npm run prototype:target -- <prototype-name>` and keep that process alive for the full review. Record the Vite port it actually binds; another session may already own the default port.
8. Before using the public Target route, follow the multi-session checks below. Then verify the exact prototype in a browser, including keyboard switching, desktop and narrow layouts, HMR, overflow, and console errors.
9. Hand over a clickable Target link only after the public route renders the requested prototype and variant.

Do not replace an existing production component with a lookalike. If the candidate needs a change to that component, make the smallest production-compatible extension and ensure every affected surface remains coherent.

## Share safely across sessions

Prototype files are ignored and worktree-local. Prototype Vite servers may run concurrently, but the current Target web service proxies all `/prototype/*` traffic to one configured Vite port. Starting another prototype server does not update that proxy, and a later Target restart can point it at a different session.

Before sharing or re-sharing a prototype:

1. Inspect listeners and prototype processes to identify the requested prototype's Vite port. Do not assume the default port or kill another session's server.
2. Inspect Target's configured `OPENAIDE_WEB_PROTOTYPE_PORT`. If it differs, report which prototype currently owns the public route.
3. Do not restart or reconfigure Target just to take ownership. Read and follow the `openaide-self-development` skill; Target runtime changes require the user's explicit request in the current conversation.
4. If the user authorizes switching Target, use the documented Target role command. Never use bare `bash deploy/local-web.sh restart`, and never touch Driver.
5. Verify through Target, not only through the loopback Vite URL. Assert prototype-specific rendered content or the exact page title; HTTP 200, the generic harness shell, or a prototype index is not proof that the requested prototype loaded.
6. If another session later changes the Target route, describe it as a routing conflict rather than a missing prototype. Keep the local prototype intact and ask before switching the public route again.

Do not claim that multiple worktree prototypes are simultaneously public. That requires slug-to-port routing, which the current single-upstream Target proxy does not provide.

## Build a logic prototype

1. Create an ignored program under `tmp/prototypes/<prototype-name>/` using the repository's existing runtime.
2. Isolate the candidate rules behind a small pure reducer, state machine, or function surface.
3. Add the smallest interactive terminal shell needed to drive meaningful actions.
4. Re-render the complete relevant state after every action so invalid transitions and surprising outcomes are visible.
5. Keep state in memory and use no production persistence unless persistence is the question.
6. Provide one command to run it and let the user drive additional cases.

The terminal shell is exploratory. Approval locks the demonstrated semantics; implement those semantics at the real product boundary with the required regression tests.

## Integrate the approved direction

1. Record the selected candidate, the reason, and the approved appearance, interactions, or semantics in the relevant task, issue, ADR, or implementation commit.
2. Implement the selected direction at the real production boundary. Preserve the approved user-visible result while adapting internal ownership, state, and interfaces as needed.
3. Add production tests at the closest real user, protocol, or storage seam and complete required error handling, persistence, logging, and accessibility.
4. Compare the integrated result with the approved prototype. If a production constraint requires visible or behavioral drift, stop and discuss it with the user.
5. Delete the ignored prototype with `npm run prototype:clean -- <prototype-name>` after the production result is verified. Logic prototypes can be deleted directly from `tmp/prototypes/`.

## Avoid these failure modes

- Making every candidate production-complete before the user has selected one.
- Reimplementing production context instead of importing it.
- Reloading or reopening the site to switch variants.
- Presenting cosmetic tweaks as distinct variants.
- Connecting a visual experiment to real destructive mutations.
- Treating approval as permission to alter the chosen UX during hardening.
- Shipping ignored fixtures, switchers, or alternate candidates as production features.
- Assuming a running prototype server is the prototype currently exposed through Target.
- Repointing Target or stopping another session's prototype without explicit user authorization.
