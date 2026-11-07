# Older App Servers rebuild a newer Chat from its Native Session

Status: accepted; supersedes the exact-edge evidence requirement of [ADR-0057](0057-desktop-update-edges-must-be-storage-compatible.md) and the alpha storage assumption of [ADR-0058](0058-windows-updates-may-launch-before-authenticode.md)

Chat storage migrations are one-way: a compaction deletes the previous generation, and [ADR-0075](0075-chat-images-are-references.md) is the first release where an older App Server cannot read a migrated Chat. Going back to an earlier release therefore needs a recovery rule. Most of a Chat is a copy of its Agent-owned Native Session, so an older App Server rebuilds what it cannot read instead of restoring a snapshot.

- **Detection.** A Chat is newer when its snapshot, a journal frame, the journal file format, or the Task's Subagent catalog carries a version above the newest this App Server knows. App Server reads those versions only after a Chat fails to load, plus the small Subagent catalog on every load. A Chat that fails for any other reason keeps the existing fail-closed path.
- **Set aside, then rebuild.** App Server moves the Chat's files, images, Tool artifacts, and Subagent histories into the Task's `set-aside/` directory, replacing any earlier set-aside. It then publishes an empty Chat and a Native Session reload requirement. Durable Task Metadata stays. The next open of the Task loads the Native Session through `session/load`, as for any deferred reload. An Archived Task waits for Restore. Nothing restores a set-aside Chat automatically; it remains only for manual recovery until replaced or the Task is deleted.
- **Durable Task Metadata stays readable.** `task.json` changes stay additive, so every older release can still read the binding it needs to rebuild. A change an older release cannot read requires its own decision.
- **What is lost.** Chat details only OpenAIDE holds: Permission and Question records, send times and durations, and Subagent histories the Agent does not replay. When the Agent no longer has the Native Session (Claude Code deletes sessions after 30 days by default), the Chat stays empty and the Task offers Reload.
- **Only what is used migrates.** Periodic storage maintenance never loads a Task this process has not loaded, so startup migrates nothing and a rollback affects only Tasks used since the update, as ADR-0075 intended.
- **Updates.** From 0.11.0, the first release with this rule, an in-app Desktop Update may cross a storage migration without exact-edge compatibility evidence or a state-root snapshot. The Windows feed keeps routing every stable source to the newest stable target. 0.10.x predates the rule: after going back to it, a Task used in 0.11 or later fails to open. That one-time break is accepted and stated in the 0.11.0 release notes.

## Considered Options

- **Snapshot the state root before a migration.** This is lossless up to the update, but it copies every Chat and its images on each migrating release, loses work done after the update, and needs restore tooling in every App Shell.
- **Reverse migrations shipped by the newer release.** No loss, but every format change needs a tested inverse for as long as rollback is supported.
- **Bridge releases that read the next format.** The older release also writes to those Tasks, so it needs the new writers and a maintained release branch.
- **Erase a newer Chat.** It is as simple as moving it aside, but a mistaken downgrade would then destroy history the newer release could still read.
