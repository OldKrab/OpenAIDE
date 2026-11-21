# Composer Images Upload Before Send

Status: accepted

On Web, Desktop, and Android, an unsent Image is uploaded through the App Server upload route when it is attached, and `task/send` carries its handle. Before this change, every Composer Image travelled base64-encoded inside `task/send`. App Server capped that at 5 MiB per Image and 8 MiB per message so one protocol message stayed bounded, which refused ordinary phone photos: eight photos from a phone camera exceed 8 MiB. The Frontend also held a base64 data URL per thumbnail, which a phone WebView struggled to draw.

## Choice

- **Route.** An Image uses the existing upload route, single request or chunked, with `X-OpenAIDE-Upload-Kind: image`. It gets the same progress, cancellation, Retry, two-at-a-time concurrency, and Send blocking as a general file.
- **Validation.** App Server reads the media type from the first bytes and accepts PNG, JPEG, GIF, and WebP. The client's file name and declared type are not trusted. An empty or unsupported file is refused when it is uploaded, not at Send.
- **Handle.** The upload becomes a pre-Send handle scoped to the client and Task, kind Image, which owns the uploaded file. Removing the row, Send, or handle expiry deletes it.
- **Send.** The draft sends uploaded Images as handles in row order beside file handles. App Server reads each file once and produces the same Image attachment an inline Image produces, so Chat storage ([ADR-0075](0075-chat-images-are-references.md)), Queue capture, and Agent delivery over ACP are unchanged. No size limit applies; the Agent enforces its own.
- **Thumbnail.** The draft row shows the picked file through an object URL instead of a base64 copy.
- **VS Code.** Its webview cannot reach the upload route, so it keeps inline Images and their limits.

## Considered Options

- **Raise or remove the inline limits.** One change, but a single protocol message would carry tens of megabytes of base64 through the reliable session and the remote-device relay, and the Frontend would still hold every Image as base64.
- **Downscale in the Frontend before Send.** Smaller messages, but the Agent receives a degraded Image the user did not ask for.
- **Upload at Send instead of at attach.** No upload for a removed Image, but Send would wait on the whole upload, and progress would appear only after pressing Send.

## Consequences

- An uploaded Image counts toward the 20-attachment limit of a message.
- No protocol type changes. The upload kind is a new HTTP header introduced with App Server Protocol 3.1, and an App Server older than 3.1 refuses a 3.1 client, so a client that uploads Images never meets an App Server that would store them as plain files.
- A lost handle (App Server restart, Prepared Task change, expiry) leaves the row visible with an error, as for files; the user attaches the Image again.
- **Edit in Composer** on a Queued Message still returns Images inline, so they meet the inline limits again when resent.
- Uploads log start and end on both sides: `attachment_upload_*` in App Server and `frontend_composer_upload_*` forwarded from Frontend, with kind, size, outcome, and duration. A refused inline Send logs `send_inline_images_rejected` with the Image count and size.

## Follow-up

- Keep the picked file in the draft and upload it again automatically when its handle is lost, instead of asking the user to attach it again.
- Upload Images returned by **Edit in Composer** so they leave the inline limits.
- Write the uploaded file into the Task's `images` directory and stream it to ACP without holding base64 in memory.
- Revoke thumbnail object URLs once the row and its pending Chat preview are gone.
