# File Viewer App Server Methods

Status: accepted

Web and Desktop talk to App Server with `fileViewer/open` (originating absolute path and optional line), `fileViewer/openFromHandle` (handle plus relative href or fragment), `fileViewer/refresh`, and `fileViewer/release`. After that first open, Frontend never reads with a raw path. The snapshot result carries handle, display path, basename, UTF-8 text or an image preview (PNG, JPEG, WebP, GIF, bounded to 2048 pixels and 2 MiB, same inspection surface as Tool image preview) or fallback/error, truncated flag, and optional language. VS Code does not use these methods.

For Web downloads (#401), the authenticated HTTP download route accepts `fileViewerHandle` with the initialized `clientInstanceId`. The existing viewer handle is reusable until release and remains bound to its owning client. `check=1` checks readability without returning file bytes, allowing the tab to report a failure before browser handoff. The transfer request reopens the current regular file, streams its bytes without preview limits, and sets an attachment filename from the referenced basename. Neither request accepts a raw path. Errors after browser handoff belong to the browser's download manager.

The snapshot `truncated` flag also identifies a reduced-resolution/static image preview. Frontend labels that preview explicitly; the original filesystem file and download handle are unaffected. File reads and conversion execute outside the shared protocol lock after authorization; completion rechecks client ownership before returning contents.
