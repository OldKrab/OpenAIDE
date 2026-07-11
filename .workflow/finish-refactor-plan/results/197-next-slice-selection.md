# Next Slice Selection: Frontend Standalone Dev Host Split

## Decision

Select the Frontend Standalone Dev Host split as the next refactor slice.

## Why This Slice

`packages/frontend/src/services/devHost.ts` is the largest remaining Frontend
production file. It currently mixes several independent responsibilities:

- standalone browser bootstrap detection;
- in-browser App Shell message routing;
- route transitions for preview surfaces;
- demo workspace, Agent, Task, Chat, native-session, config-option, tool-detail,
  and Settings data construction.

This is a real Frontend/App Shell boundary for local browser preview. It is not
the final Web App App Server integration, but it should still be modular because
it exercises shared Frontend without VS Code APIs and will remain useful while
the Web/Desktop shells are developed.

## Scope

- Keep `standaloneBootstrap()` and `createStandaloneHost()` importable from
  `services/devHost.ts`.
- Split demo data factories away from host message routing.
- Split standalone bootstrap/path helpers away from host message routing.
- Split message routing into a focused module that converts
  `WebviewToHostMessage` requests into current demo `HostToWebviewMessage`
  responses.
- Preserve all current demo data, route behavior, async `post` timing, message
  types, payload shapes, and `hostBridge.ts` behavior.

## Out Of Scope

- No real Web App App Server transport.
- No App Server Protocol changes.
- No App Shell contract changes.
- No visible UI redesign.
- No changes to VS Code extension host behavior.

## Risks

- The standalone host currently relies on browser globals and path inspection;
  moving bootstrap logic can accidentally enable preview mode inside VS Code
  webviews.
- Demo message routing has several response metadata fields such as request ids,
  append flags, and snapshot intents that must be preserved.
- Route transitions use `window.history.pushState` plus reload; changing that
  would alter preview behavior.

## Next Step

Record and commit the accepted API contract, then implement this slice.
