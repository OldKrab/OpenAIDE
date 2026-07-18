# `.worktreeinclude` reference behavior and Rust reuse options

Research date: 2026-07-16

## Scope

This note inspects `satococoa/git-worktreeinclude` at commit [`e8443611`](https://github.com/satococoa/git-worktreeinclude/tree/e8443611dd6471960acade5685f0ca755a523646) and compares its implementation shape with available Rust copy crates. The goal is behavioral compatibility for OpenAIDE, not source translation.

## Reference behavior

The reference tool treats `.worktreeinclude` as a Git-ignore-compatible pattern file. It computes the intersection of:

1. untracked paths Git classifies as ignored under the repository's normal exclusion rules; and
2. untracked paths Git classifies as ignored by `.worktreeinclude`.

It obtains both sets with NUL-delimited `git ls-files -o -i` calls, using `--exclude-standard` for the first set and `-X <include-file>` for the second. Tracked files therefore never enter the copy plan. Pattern parsing, nested ignore behavior, negation, and path quoting remain Git's responsibility rather than a custom matcher. [Reference README](https://github.com/satococoa/git-worktreeinclude/blob/e8443611dd6471960acade5685f0ca755a523646/README.md), [engine source](https://github.com/satococoa/git-worktreeinclude/blob/e8443611dd6471960acade5685f0ca755a523646/internal/engine/engine.go)

Other observed semantics from the engine source:

- The default source is the first non-bare entry from `git worktree list --porcelain -z`, normally the main worktree. An explicit source must resolve to the same canonical Git common directory as the target.
- A missing source `.worktreeinclude` is successful no-op behavior.
- Repository-relative paths are normalized and checked against source and target roots.
- Each matched path is inspected with `lstat`. Only regular files are copied. Symbolic links and other non-regular paths are skipped rather than followed, recreated, or treated as fatal errors.
- Parent directories are created with mode `0755`. File copies preserve ordinary permission bits from the source.
- Each file is copied to a temporary sibling and renamed into place. Atomicity is per file, not for the complete include operation.
- Existing equal files are skipped after size and SHA-256 comparison. Differing targets are conflicts unless force-overwrite is requested.
- Processing continues after individual file errors. Earlier successful copies remain in place, so an error can produce a partial result. There is no transaction or rollback.
- The reference implementation has no aggregate byte, file-count, or individual-file size limit.
- Machine-readable output contains relative paths and statuses but never file contents.

## Rust reuse options

No inspected Rust crate implements the full `.worktreeinclude` contract. Generic copy crates solve a different problem because they traverse directories themselves, while the reference semantics rely on Git to produce the exact eligible file set.

- [`recursive_copy`](https://docs.rs/recursive_copy/latest/recursive_copy/) supports configurable symlink handling and permissions but would still require separate Git-compatible matching and tracked/ignored filtering.
- [`fs-more`](https://docs.rs/fs-more/latest/fs_more/directory/fn.copy_directory.html) offers detailed directory-copy behavior, including preserved symlinks, but recursive directory copying is broader than the reference regular-file plan.
- [`parcopy`](https://docs.rs/parcopy/latest/parcopy/) provides parallel and per-file atomic copying, but adds traversal, symlink, resume, and concurrency policy that OpenAIDE does not need for compatibility.

OpenAIDE already depends on `tempfile`, while Git is necessarily available for worktree creation. The smallest compatible implementation is therefore a focused Rust module that:

1. invokes Git for NUL-delimited eligible-path discovery exactly as the reference tool does;
2. intersects and sorts those paths;
3. validates roots and uses `symlink_metadata`/`lstat` semantics;
4. copies regular files with a temporary sibling and atomic rename; and
5. returns a structured per-path summary and a failed overall preparation result when errors occur.

Adding a generic recursive-copy dependency would not remove the security-critical Git classification or path validation work and would introduce behavior that must then be disabled or adapted. Behavioral reimplementation behind a small module is preferable to vendoring the Go program or adopting a mismatched traversal crate.

## OpenAIDE-specific source choice

The reference tool's default source is the first non-bare worktree. OpenAIDE intentionally differs here: Managed Worktree creation copies from the Project checkout that initiated creation, including when that Project is itself a linked worktree. This makes the local environment files selected by `.worktreeinclude` follow the user's visible source context rather than an unrelated repository-level default.
