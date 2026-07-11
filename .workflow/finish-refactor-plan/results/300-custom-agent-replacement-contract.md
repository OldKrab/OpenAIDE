# 300 Custom Agent Replacement Contract

## Scope

Next A6 slice: remove generic in-place Custom Agent save semantics for edits
that change how the Agent launches.

## API Contract

- `agent/createCustom` creates a new Custom Agent identity from label, icon,
  command line, parsed launch command/args, env, secret env, and enabled state.
- `agent/updateCustomMetadata` updates only non-launch Custom Agent metadata:
  label, icon, and enabled state.
- `agent/replaceCustom` handles launch-affecting edits for an existing Custom
  Agent. Params include the source Agent id, full replacement launch/settings
  data, and an explicit confirmation flag.
- `agent/replaceCustom` creates a fresh Custom Agent id, removes the old Custom
  Agent catalog record, clears old and new status/probe cache entries, replaces
  the live registry atomically, and returns old/new ids plus the Agent
  collection snapshot.
- App Server rejects launch replacement without the confirmation flag. Frontend
  must show a warning before calling it.

## Launch-Affecting Fields

- `commandLine`
- parsed `command`
- parsed `args`
- `env`
- `secretEnv`
- transport, if more than stdio is added later

## Non-Launch Fields

- `label`
- `icon`
- `enabled`

## Frontend Behavior

- Creating a Custom Agent calls `agent/createCustom` immediately on save.
- Editing label/icon/enabled only calls `agent/updateCustomMetadata`.
- Editing command/env on an existing Custom Agent requires an explicit second
  confirmation action and then calls `agent/replaceCustom`.
- Replacement is treated like a create plus delete in Settings state: select the
  new Agent id and remove the old Agent id from Agent details/snapshot.

## Current Limit

No durable Task history rewrite is part of this slice. Historical Tasks keep the
Agent id/name recorded when they were created; future work can add explicit
continue/fork recovery for Tasks bound to removed Agent identities.

## Implementation Result

- Replaced the generic App Server `agent/saveCustom` method with
  `agent/createCustom`, `agent/updateCustomMetadata`, and `agent/replaceCustom`.
- Removed Frontend/App Shell generic custom Agent save fallback messages and
  carried the split through Settings callbacks and Agent Settings UI props.
- `agent/replaceCustom` requires explicit confirmation and also rejects
  metadata-only replacements even when a client sends confirmation.
- Metadata-only updates no longer parse or validate launch command text.
- Frontend warning UI still presents one Save button, but dispatches explicit
  create, metadata update, or replacement callbacks.

## Review Fixes

- Fixed App Server replacement validation so non-launch edits cannot create a
  fresh Agent identity through `agent/replaceCustom`.
- Removed legacy `agent.custom.save` shell fallback and save-result handling.
- Split generic Frontend save contracts into explicit create/update/replace
  callbacks.
- Shared Frontend launch-change comparison between confirmation and intent
  method selection.
- Kept changed production source files below the project size limit.

## Verification

- `$doomsday-review` correctness, requirements/tests, and code-quality passes
  with all material findings fixed.
- `cargo fmt --all --check`
- `cargo test -p openaide-runtime`
- `cargo test -p openaide-app-server-protocol`
- `npm run protocol:check`
- `npm run build --workspace @openaide/app-shell-contracts`
- `npm run build --workspace @openaide/app-server-client`
- `npm run check --workspace openaide-frontend`
- `npm run test --workspace openaide-frontend`
- `npm run check --workspace openaide-vscode-extension`
- `npm test --workspace openaide-vscode-extension -- src/webview/messaging.test.ts`
- `npm run test --workspace @openaide/app-server-client`

## Next

Select and grill the next A6 slice after Custom Agent replacement.
