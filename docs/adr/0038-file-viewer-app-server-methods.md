# File Viewer App Server Methods

Status: accepted

Web and Desktop talk to App Server with `fileViewer/open` (originating absolute path and optional line), `fileViewer/openFromHandle` (handle plus relative href or fragment), `fileViewer/refresh`, and `fileViewer/release`. After that first open, Frontend never reads with a raw path. The snapshot result carries handle, display path, basename, UTF-8 text or an image preview (PNG, JPEG, WebP, GIF, 5 MB, same inspection surface as Tool image preview) or fallback/error, truncated flag, and optional language. VS Code does not use these methods.
