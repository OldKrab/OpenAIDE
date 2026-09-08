# Managed Codex ACP session recovery patch

The pinned `@openaide/codex-acp@1.2.1` adapter always supplies a provider when resuming or loading a session. Native Codex treats that as an explicit override and skips its persisted model/reasoning restoration. The adapter also rebuilds approval mode from its startup default. Native Codex 0.153.3 restores approval policy and reviewer but reloads the sandbox from current configuration, so forwarding its resume response alone can broaden an existing session's permissions.

`apply-session-recovery.mjs` changes these resume/load boundaries. The helper reads the latest policy from the native-owned history path before resume can append new settings. Explicit provider overrides remain effective. Recovered full policies appear as **Native session policy**, and subsequent prompts keep the native process's resolved permissions until the user explicitly chooses another preset. This avoids replacing granular rules with the legacy sandbox projection. New sessions and explicit preset changes keep their existing behavior. The patch does not replay OpenAIDE's stale option catalogs.

This is a version-scoped compatibility reader for native storage. It recognizes both older turn contexts and newer settings records, with the latest record authoritative. Restricted managed profiles use native named-permission configuration. Canonical workspace profiles first resume read-only, then apply their original workspace settings and await the native completion notification. This retains protected metadata rules whose `missing_path_behavior: "skip"` cannot be expressed by named configuration, including external Git worktree metadata. The staging resume leaves the original durable policy intact if the process fails before the update.

Malformed historical JSON records may be superseded only by a later complete native turn context with explicit permissions, approval policy/reviewer, cwd, and workspace roots. Corruption after the last complete context remains blocking; recovery never falls back to older permissions. The native history file is not modified. Metadata-only policy-read events report the malformed-record count and terminal outcome.

Unknown or corrupt profiles fail before resume; they require selecting a supported permission profile in native Codex before reopening. Unsupported shapes include custom missing-path rules, external or unrestricted managed sandboxes, and custom enabled-network profiles whose proxy restrictions cannot be reconstructed. They must never be rounded to a more permissive preset. Existing Chat and native history remain intact on that failure. The reader validates native session identity and UTF-8, streams one record at a time, and reports errors without history content or paths.

If settings restoration fails, the helper releases its native subscription. Native unsubscribe does not cancel a pending settings update, so an uncertain result blocks reuse of that session on the same connection and requires an OpenAIDE App Server restart. A definite rejected request can be retried by reopening. Settings restoration logs safe start/terminal events, duration, and classified errors.

The installer accepts only the exact package version and upstream bundle digest in `session-recovery-manifest.json`. It writes into an unpublished staging directory, verifies both output digests, and uses a distinct managed runtime cache revision so an old App Server can retain its leased installation. The npm lockfile remains the upstream distribution pin.

Remove the patch when the pinned dependency fixes native recovery. Any interim change requires updating the reviewed helper/output digests and advancing `runtimeId`; do not modify a published cache directory in place. An upstream package update must either remove this patch or deliberately regenerate its pins.

Validation:

- `node --test scripts/codex-session-recovery.test.mjs` checks offline version, upstream-integrity, and artifact-corruption failures.
- `node --test openaide-rs/app-server/assets/codex-acp-runtime/session-recovery.test.mjs` checks policy reconstruction, malformed history, and bounded settings completion. Both offline suites run in `npm run test:gate`.
- `node scripts/smoke-codex-session-recovery.mjs <adapter-entrypoint>` exercises the actual adapter with an isolated native JSON-RPC fixture. It covers resume/load, explicit providers, and exact policies through the next prompt, without model calls.
- `node scripts/smoke-codex-native-recovery.mjs <native-codex-binary> <adapter-entrypoint>` checks full native policy and workspace scope across two process restarts and new turns. Its isolated local provider makes no model calls.
- `scripts/smoke-packaged-codex-acp.mjs` runs both behavioral checks against the runtime provisioned by the packaged App Server on POSIX.
