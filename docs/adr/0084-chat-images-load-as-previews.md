# Chat Images Load as Previews

Status: accepted

A Chat row loads a small preview of a stored image, and the full-size view loads the original. Before this change, `task/image` ([ADR-0075](0075-chat-images-are-references.md)) returned only the stored bytes. A phone screenshot is about 1 MB, so a Task with three screenshots sent 3.6 MB of base64 when it opened. Remote Devices reach a computer through a relay when they cannot connect directly, and the free public relays measured about 45 KB/s, which turned those three images into more than a minute of transfer. Requests behind them waited, and the phone dropped the connection. A Chat row draws an image at most 320 CSS pixels wide and 240 tall, so almost all of those bytes were never shown.

## Choice

- **Request.** `task/image` takes an optional `variant`: `preview` or `original`. Absent means the original, so a client from before this change keeps working; App Server Protocol 4.1 adds the field.
- **Preview.** At most 960 pixels on the longest edge, which stays sharp at 320 CSS pixels on a 3x screen, after EXIF orientation. An opaque image becomes a JPEG at quality 80. An image with visible transparency stays PNG, because flattening it would pick a background the theme did not choose. Screenshots that carry an unused alpha channel count as opaque. An encoding over 256 KB is halved until it fits.
- **The original instead.** App Server returns the stored bytes when the image is already within the bounds, when the preview would not be smaller, or when it cannot decode the format. A small image keeps its animation and exact colors, and no client ever receives more bytes for a preview than for the original.
- **Result.** The result carries `mediaType` only when the bytes are a preview encoded again. Otherwise the bytes have the media type the reference already names.
- **Where the work runs.** The access check and the file read stay under the protocol lock, as before. Decoding and shrinking run after the lock is released, under the decode gate the File Viewer image previews already use, so a large screenshot does not stall other clients' requests and events. The answer is dropped if the connection changed owner meanwhile.
- **Server cache.** App Server keeps previews in memory by image id, up to 16 MB, least recently used first. An id is the SHA-256 of the stored bytes, so an entry never goes stale. Reopening a Task, or opening it on another device, does not decode its images again.
- **Client cache.** The Frontend cache ([ADR-0075](0075-chat-images-are-references.md)) holds each variant under its own key.
- **Full-size view.** Opening an image shows its preview at once, at the original's size from the reference, and loads the original. When it arrives it replaces the preview without moving the view. While it loads the toolbar says so; if it fails the preview stays, with Retry.

## Considered Options

- **Make previews when an image is stored.** No decode on request, but every stored image costs a second file whether or not it is shown, and images stored before this change would still need previews made on request.
- **Store previews on disk beside the images.** They would survive a restart, but the unreferenced-image sweep and Task deletion would have to know about a second kind of file. A restart costs one decode per image shown again.
- **Let the client ask for a pixel size.** A preview could match each screen, but every size would be a separate cache entry on both sides. One preview size covers every Chat row.
- **Lossy WebP.** About half the size of JPEG at the same quality, but the image library App Server uses encodes WebP only losslessly, and a second encoder is a new native dependency.

## Consequences

- Inline images in Chat are slightly softer on wide desktop screens than the original. The full-size view shows the original.
- Animated GIFs larger than the bounds show their first frame in Chat and animate in the full-size view.
- Image loads log the variant on both sides. App Server logs `task_image_started` and then `task_image_served` with the source and sent sizes, whether the preview was reduced, whether it came from the cache, the time spent waiting for the decode gate, and the reason it fell back to the original. Frontend logs `chat_image_load_*` with the variant and whether the bytes were reduced.
- File Viewer image previews now also turn opaque images with an unused alpha channel into JPEG instead of PNG, because both use the same reduction.
- On the three phone screenshots that prompted this change, previews were 238 KB against 2.79 MB of originals, made in about 50 ms each in a release build.
