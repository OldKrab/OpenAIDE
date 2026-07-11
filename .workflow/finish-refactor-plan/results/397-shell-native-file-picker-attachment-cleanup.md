# P397 Shell-Native File Picker Attachment Cleanup

## Result

- Removed `context.pickFile` and `context.file.result` from the webview shell contract.
- Removed the VS Code shell-native picker handler and obsolete workspace file picker helper.
- Removed Frontend host-message ingestion for shell-returned attachment objects.
- Removed composer callback plumbing that could post shell file-pick messages.
- Kept Task composer file attachment on the App Server-backed file browser path, which creates App Server-owned attachment handles.
- Disabled Attach file in composer contexts where no App Server file browser is available.

## Verification

- `npm run check --workspace @openaide/app-shell-contracts`
- `npm run check --workspace openaide-frontend`
- `npm run check --workspace openaide-vscode-extension`
- `npm run test --workspace openaide-frontend -- ComposerView.test.tsx AppSurfaces.test.tsx appControllerCallbacks.test.ts hostMessageRouter.test.ts`
- `npm run test --workspace openaide-vscode-extension -- messaging.test.ts`
- Active source scan for `context.pickFile`, `context.file.result`, `pickWorkspaceFile`, `onPickFileContext`, and `pickFileContext`
