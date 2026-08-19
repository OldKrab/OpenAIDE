# File Viewer Handles And Shell Open Path

Status: accepted

A Web or Desktop user click may send the originating absolute path to App Server once, because that path is already visible in Chat or tool detail. App Server returns an opaque viewer handle even when the read fails, so Retry, Refresh, and relative Markdown opens are handle-only. Handles are released when the File Tab closes, the last tab is dismissed, the user leaves the Task Page, or the client detaches; collapse does not release them. VS Code keeps `tool.openPath` into the editor and does not use this File Viewer. Web and Desktop must not implement `tool.openPath` as a filesystem read in the App Shell.
