# P399 Custom Agent Launch Replacement API

## Decision

Launch-affecting Custom Agent edits are a distinct App Server workflow:

- Metadata-only edits use `agent/updateCustomMetadata`.
- Launch-affecting edits use `agent/replaceCustom`.
- Replacement is one confirmed mutation, not a two-step Backend prepare/confirm flow.
- Frontend may show the first-click warning locally for responsiveness, but Backend is authoritative and must reclassify the change against stored Agent data.

## Launch Identity

Launch-affecting fields are:

- transport
- command line and parsed command
- args
- plain env names and values
- secret env names

Metadata-only fields are:

- label
- icon
- enabled

Backend must compare normalized stored launch identity with the submitted replacement. If launch identity did not change, `agent/replaceCustom` rejects and Frontend should use metadata update instead.

## Protocol Contract

Keep the existing method names:

- `agent/updateCustomMetadata`
- `agent/replaceCustom`

`agent/replaceCustom` params:

- `sourceAgentId`
- full replacement metadata and launch fields
- `confirmation.acceptedLaunchIdentityChange`

`agent/replaceCustom` result should include:

- `oldAgentId`
- `newAgentId`
- `agents`
- `cleanup`

`cleanup` is render-safe metadata owned by Backend. Initial shape:

- `removedCatalogRecord: boolean`
- `removedCachedStatus: boolean`
- `removedSettingsOverlay: boolean`
- `historyPolicy: "preserveHistoricalTasks"`

## Backend Semantics

- Generate a new custom Agent id for the replacement. Do not let Frontend choose it.
- Remove the old custom catalog record.
- Remove App Server-owned mutable overlays/caches for the old id where they exist.
- Do not rewrite historical Task records, Chat, Native Session ids, or old Agent labels already committed to Task history.
- Old Tasks remain renderable using recorded Agent id/label and may be unavailable for new sends if that Agent identity no longer resolves.
- Publish `AgentCollectionUpdated` after the committed replacement.
- Reject replacements for built-in Agents or missing custom Agents.
- Reject replacement without confirmation when Backend detects a launch identity change.

## Frontend Semantics

- UI keeps immediate first-click warning before calling Backend.
- After result, select/render `newAgentId`.
- Show stable errors from Backend if the stored Agent changed since the UI loaded.
- Do not derive cleanup behavior or old Task history policy locally.

## Next Packet

P400 should implement the missing protocol/storage pieces:

- Add `AgentReplaceCustomCleanup` and `historyPolicy` result typing.
- Move new-id generation fully into Backend if any Frontend-selected id remains.
- Ensure replacement cleanup removes known old-id mutable Agent cache/overlay state.
- Add Rust and Frontend tests for confirmation, same-launch rejection, cleanup result, and historical Task preservation policy.
