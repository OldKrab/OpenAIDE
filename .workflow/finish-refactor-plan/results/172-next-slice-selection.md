# Next Slice Selection: Frontend Agent Settings Tab Split

## Selected Slice

Split `packages/frontend/src/components/settings/AgentSettingsTab.tsx` into
focused Settings subcomponents while keeping `AgentSettingsTab.tsx` as the
public Settings tab component.

## Why This Slice

`AgentSettingsTab.tsx` is now the largest Frontend production component at 375
lines and is close to the source-size limit. It combines several local
responsibilities:

- Agent master-list rendering and selection;
- Agent detail header and authentication action rendering;
- built-in Agent status, launch, and availability sections;
- custom Agent draft editing;
- custom Agent icon picking;
- custom Agent environment editing;
- save/delete acknowledgement helpers.

The split advances the accepted Frontend architecture by keeping the Settings
tab shell small and making leaf UI responsibilities explicit without changing
Backend, protocol, host bridge, or App Shell behavior.

## Candidate Shape

- Keep `AgentSettingsTab.tsx` as the public tab component and local state owner.
- Extract focused local modules under `components/settings/agent/` or adjacent
  Settings files:
  - Agent list;
  - Agent detail header/status sections;
  - custom Agent draft form;
  - custom Agent environment editor;
  - custom Agent icon picker;
  - pure draft/acknowledgement helpers.
- Keep `shouldConsumeAgentSaveAck` and `shouldConsumeAgentDeleteAck` exported
  from `AgentSettingsTab.tsx` unless tests are updated to import them from a
  helper module and `SettingsView.test.tsx` remains focused.

## Explicit Non-Goals

- Do not change visible Settings text, class names, ARIA labels, tab behavior,
  callback props, custom Agent save/delete payloads, or authentication behavior.
- Do not add host bridge, App Server client, reducer, protocol, or shell imports
  to Settings leaf components.
- Do not redesign Agent settings UX in this slice.
- Do not split `Composer.tsx` in this slice.

## Risks To Grill

- Draft state must stay in the tab owner so save/delete acknowledgements still
  reconcile with the initiating draft.
- The double-click-style custom Agent delete confirmation must keep the same
  pending and confirmation behavior.
- Built-in Agent enable toggles and custom Agent enabled draft toggles must stay
  distinct.
- Authentication buttons in the detail header and status panel must keep the
  same disabled/auth method behavior.

## Next Step

Grill and record the API contract for the Frontend Agent Settings tab split
before implementation.
