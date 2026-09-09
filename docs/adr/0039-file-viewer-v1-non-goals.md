# File Viewer V1 Non-Goals

Status: accepted

Project files expands the initial File Viewer with directory browsing, filename/content search, working-change diff, and text/range File Quotes. Editing, staging, commits, push, persisted review threads, hex view, live watching or auto-refresh, persisted File Tabs, archive-time snapshots, File Viewer inside VS Code, native save-as, and immediate sending of File Quotes remain outside this boundary.

Issue #401 adds Download to the Web File Viewer tab, including unsupported files and preview errors. Download streams the current filesystem bytes independently of preview limits and preserves the referenced basename, including symlink aliases and Unicode subject to browser/OS restrictions. The existing client-bound file authority applies, including explicitly selected paths outside the workspace; only readable regular files download. Browser download UI owns transfer progress, while failures to start appear in the tab with an explicit retry. Desktop and VS Code behavior is unchanged.

In Web Chat, explicit Markdown file links accept binary extensions, spaces, and extensionless names. Inline-code path detection remains conservative, and external URLs retain browser behavior. A file reference is a path to the current file, not a stored historical artifact.
