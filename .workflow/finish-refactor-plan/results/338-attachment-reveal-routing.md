# P338 Attachment Reveal Routing

## Result

- Added typed `attachment/reveal` protocol with generated TypeScript bindings.
- App Server resolves revealable file-reference attachment handles to App Server-owned shell reveal handles.
- `attachment/reveal` emits a same-client `shell/revealFile` request using only opaque `fileHandleId` and safe label.
- Embedded snapshot handles are not revealable.
- Frontend composer attachment tokens expose reveal only for App Server handle-backed attachments.

## Verification

- `cargo fmt --all --check`
- `cargo check -p openaide-runtime`
- `cargo test -p openaide-app-server-protocol attachment -- --nocapture`
- `cargo test -p openaide-app-server-protocol methods -- --nocapture`
- `cargo test -p openaide-runtime protocol_edge::tests::attachment_reveal_opens_same_client_shell_reveal_request_with_opaque_handle -- --nocapture`
- `cargo test -p openaide-runtime attachment_runtime -- --nocapture`
- `npm run protocol:generate`
- `npm run protocol:check`
- `npm run check --workspace @openaide/app-server-client`
- `npm run check --workspace openaide-frontend`
- `npm run test --workspace openaide-frontend -- ComposerView.test.tsx appControllerCallbacks.test.ts AppSurfaces.test.tsx`
- `npm run check`
- `git diff --check`
- Production source size scan excluding tests, generated files, examples, and `node_modules`

## Remaining

- A9 has no known remaining items.
