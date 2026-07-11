# P336 Frontend File Browser Composer

## Result

- Task composer Add context can open an App Server-backed workspace file browser when a typed BackendConnection is available.
- File rows expose explicit `Reference` and `Embed` actions.
- Rendering components call typed task callbacks instead of protocol methods directly.
- App Server pre-send handles are added to composer state without raw local paths.
- The browser renders loading and recoverable error states for slow or failed local work.

## Verification

- `npm run check --workspace openaide-frontend`
- `npm run test --workspace openaide-frontend -- ComposerView.test.tsx appControllerCallbacks.test.ts AppSurfaces.test.tsx`

## Remaining

- Attachment runtime TTL cleanup.
- Live open/reveal routing for attachment handles.
