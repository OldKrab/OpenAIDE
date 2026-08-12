# Sandbox Mermaid Diagram Rendering

OpenAIDE renders explicit `mermaid` fences from ordinary Agent Chat text as an ephemeral Frontend projection of the durable source. The official Mermaid package runs only inside a packaged sandboxed renderer with scripts enabled, no same-origin authority, and no network access; the parent displays its returned SVG as a static data-image and never binds Mermaid interactions. This isolates Mermaid's required transient DOM and inline styles from the main application CSP, prevents author-controlled styles or resources from entering the OpenAIDE document, and avoids persisting generated markup tied to a Mermaid version.

## Considered Options

Direct inline SVG, including SVG sanitized a second time by OpenAIDE, was rejected because Mermaid inserts generated styles before drawing and the main VS Code webview correctly forbids inline styles. Weakening the main CSP with `unsafe-inline` was rejected because it broadens the application trust boundary. A React Mermaid wrapper was rejected because it does not address rendering isolation, CSP, streaming, or accessibility.

## Consequences

App Shells must package and admit only the isolated renderer required by shared Chat; failure leaves the original source visible instead of creating shell-specific behavior. The Web parent loads its authenticated packaged assets and transfers the bundle as encoded data into a self-contained sandbox document, because an opaque child cannot use the parent's authentication cookie. VS Code instead admits its packaged renderer resource directly. The renderer uses strict Mermaid security, disables HTML labels and callbacks, applies resource limits, and returns no executable behavior. The outer image uses authored Mermaid accessibility metadata when available and otherwise directs assistive-technology users to the complete source. Zoom and pan remain ephemeral presentation state owned by Frontend. Theme changes regenerate the ephemeral image, and packaged-browser verification must cover both the default and VS Code paths.
