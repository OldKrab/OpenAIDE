# Target Stack

OpenAIDE will use a standalone Rust runtime for the JSON-RPC backend and a TypeScript VS Code extension host with a TypeScript webview UI. This keeps runtime orchestration reusable outside VS Code while keeping host integration, packaging, and editor/webview behavior aligned with the VS Code platform; the trade-off is extra bridge and build complexity compared with an all-TypeScript implementation.
