# Frontend Agent Settings Tab Split: Implementation

## Scope

Implemented the accepted Frontend Agent Settings tab split without changing the
public Settings tab component, props, callback behavior, or helper import path.

## Changes

- Kept `AgentSettingsTab.tsx` as the public Settings tab component and local
  state owner for selected Agent, custom Agent draft, delete confirmation,
  pending save id, and pending delete id.
- Added `agentSettingsModel.ts` for pure draft helpers, acknowledgement
  predicates, `AgentDraft`, and Agent status copy.
- Added `AgentSettingsList.tsx` for the Agent master list and Add action.
- Added `AgentSettingsDetail.tsx` for the detail header, status panel,
  built-in launch rows, availability toggle, custom Agent fields, and actions.
- Added `AgentCustomFields.tsx` for icon picker and environment editor leaf
  rendering.
- Preserved `shouldConsumeAgentSaveAck` and `shouldConsumeAgentDeleteAck`
  imports from `AgentSettingsTab.tsx` via re-export.
- Added mounted `AgentSettingsTab.test.tsx` coverage for custom Agent save
  payloads, custom Agent delete confirmation, and built-in Agent enable
  toggles.

## Preliminary Verification

- `npm run check --workspace openaide-frontend`
- `npm run test --workspace openaide-frontend -- SettingsView.test.tsx AgentSettingsTab.test.tsx`
- `git diff --check`
- Source-size scan for Settings modules:
  - `AgentSettingsDetail.tsx` 180 lines;
  - `AgentSettingsTab.tsx` 116 lines;
  - `AgentCustomFields.tsx` 75 lines;
  - `agentSettingsModel.ts` 62 lines;
  - `AgentSettingsList.tsx` 52 lines.
- Boundary scan found no host bridge, App Server client, reducer, App
  controller, service, or protocol imports in Settings modules.

## Next Step

Run `$doomsday-review` for the slice, fix material findings, then run
integration verification before committing.
