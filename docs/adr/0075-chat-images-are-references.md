# Chat Images Are References

Status: accepted

A sent Image and an Agent-returned image are stored once as a file in their Task's directory. Chat stores and sends only a reference, and Frontend loads the bytes when the image is shown. Before this change, every image was base64 inside the stored Chat. That copy was held in App Server memory, rewritten by every history write, and sent in every Task baseline, Chat page, and Chat change that touched its message. A Task with a few screenshots therefore cost tens of megabytes on every switch and every streamed update to that message.

## Choice

- **Scope.** User message Image attachments and Agent message image parts, in a Task's Chat and in its Subagent histories. Tool detail images are unchanged, because Tool details are already stored as separate artifacts and loaded per Tool when expanded. Queued Message attachments are unchanged, because they are never sent in a snapshot.
- **Identity.** An image's id is the SHA-256 of its bytes. The id never names different bytes, so a file is written once and a client copy never goes stale. The same image sent twice, or replayed by a Native Session reload, is one file.
- **Storage.** Images live in the Task's `images` directory, shared by the Task's Chat and Subagent histories, so Task delete, retention cleanup, and Reset task history remove them with the Task. An image file is synced before the Chat frame that references it, the rule [ADR-0029](0029-split-task-metadata-and-chat-storage.md) already applies to Tool artifacts. A crash between the two leaves an unreferenced file, which compaction removes along with any file no Chat item or Subagent history references. Only files older than a day are removed, so a write whose frame has not committed yet keeps its file, and Subagent histories are read only when an old file is no longer referenced by Chat. The Task journal writer is the only code that stores images: a change drafted before the commit publishes the reference its bytes will be stored under, which the content hash fixes in advance, so a new Task's directory is never created ahead of its first frame.
- **Reference.** The stored and protocol forms carry the image id, its media type, and its size in bytes, so Frontend can reserve the image's footprint before loading it. They no longer carry a data URL.
- **Loading.** A new App Server request, `task/image`, returns one image of one Task by id as base64 bytes; the reference that named it already carries the media type. It checks Task access exactly like Tool image previews and reads only from that Task's `images` directory. It uses the existing connection, so every App Shell loads images the same way, including VS Code and Desktop, whose content policies do not allow images from an App Server URL.
- **Client cache.** Frontend keeps loaded images in memory, bounded in bytes and evicting the least recently shown first, and shares one request among every view of the same image. Because an id never changes meaning, a cached image is never revalidated. While an image loads, its footprint shows a loading state; a failure shows an explicit unavailable state with Retry, and the rest of the message renders normally.
- **Migration.** The Chat storage version increases, with a fixture in the previous format, as ADR-0029 requires. A Chat or Subagent history that still holds inline images migrates at its normal hydration boundary: App Server writes its image files, then publishes a compacted generation that holds only references. A failed or interrupted migration leaves the previous generation authoritative and retryable. Unopened Tasks are not touched, and startup does not migrate.
- **Protocol version.** A client that does not know references would show Chats without images, so the App Server Protocol major version increases to 3.0, as [ADR-0062](0062-remote-devices-are-app-server-clients.md) requires. An older client on another device is refused with a request to update.

## Considered Options

- **Leave stored images inline and send only references.** No migration, but App Server would keep holding and rewriting the bytes on every history write, and would serve images by searching stored messages.
- **Migrate an old history only when it is next rewritten for another reason.** It writes nothing extra, but until that rewrite App Server would need a second way to serve an image from inline bytes. Migrating at hydration, like every earlier storage change, keeps one path.
- **Migrate every Task at startup.** One pass, but a slow first start after upgrading, spent on Tasks that may never be opened again.
- **Serve images over HTTP.** The browser would cache them itself, but VS Code would still need the extension to relay the bytes, and Desktop's content policy would have to admit the App Server origin. That is a second path per App Shell.
- **Keep sending inline images to clients that do not declare reference support.** Older clients would keep working, but every snapshot, page, change, and Subagent history projection would need a second form until the next major version.

## Consequences

- A release with this migration is not an eligible in-app Desktop update edge under [ADR-0057](0057-desktop-update-edges-must-be-storage-compatible.md), because an older App Server cannot read a migrated Chat.
- The Composer draft is unchanged: unsent Images stay in Frontend, and Send still carries them inline. App Server writes their files when it accepts the message. **Edit in Composer** on a Queued Message still returns the bytes.
- Image loads log their outcome, size, duration, and classified error on both sides, keyed by Task id and image id, so a missing image can be traced from the Frontend request to the file read.
