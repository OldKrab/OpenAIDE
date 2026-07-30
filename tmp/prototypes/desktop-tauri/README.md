# PROTOTYPE — Tauri desktop shell

Question: can OpenAIDE reuse its shared React work surface while Tauri owns
desktop lifecycle, native commands, and App Server launch?

This directory is ignored throwaway code. It must not be promoted into
production. Run it from the repository root:

```sh
tmp/prototypes/desktop-tauri/run.sh
```

On Windows PowerShell:

```powershell
tmp\prototypes\desktop-tauri\run.ps1
```

The command builds the existing App Server, installs the prototype-only
dependencies when needed, and starts `tauri dev`.

The spike intentionally proves only:

- shared Frontend rendering as `shellKind: "desktop"`;
- authenticated App Server attach-or-launch;
- native File/Edit/View/Help menus and accelerators;
- a native folder picker;
- a native test notification;
- desktop navigation between New Task, Settings, and existing Tasks.

It does not prove signed macOS/Windows packaging, credential storage, updater
behavior, screen-reader quality, or production recovery.

On a non-Windows machine, this compile-only check can validate the Windows Rust
surface without linking an installer:

```sh
PATH="$PWD/tmp/prototypes/desktop-tauri/tools:$PATH" \
  cargo check \
  --manifest-path tmp/prototypes/desktop-tauri/src-tauri/Cargo.toml \
  --target x86_64-pc-windows-gnu
```

The prototype-local `windres` shim intentionally does not produce a resource
file. A real Windows build must run on Windows with its actual resource compiler.
