# ACP Question UI prototype

Question: what should a session-scoped ACP form elicitation look and feel like inside an OpenAIDE Task?

This is throwaway UI. It does not use the App Server and must not be promoted directly into production code.

Scene: a developer is reviewing a blocked Agent turn in a dim VS Code workbench and needs to answer without losing the surrounding execution context. This keeps the existing dark editor theme and restrained color strategy.

Variants:

- `A`, Workbench form: the selected direction. A soft tonal surface, borderless choices, and rounded filled controls make the request lifecycle explicit without stacking outlined rectangles.
- `B`, Conversational fields: document-like questions with minimal container chrome.
- `C`, Compact worksheet: a dense two-column layout for forms with several fields.

Review feedback: keep Variant A's border-light unified rounded surface, remove the field numbers, and separate fields with whitespace plus quiet horizontal rules. Separate tonal field cards were rejected because they broke the form into too many containers.

Use the bottom switcher or `?variant=A|B|C`. The `state` query parameter supports `pending`, `error`, `sending`, `resolved`, and `closed`.

Run and publish to the disposable target with:

```sh
npm run web:target
```

After review, record the winning structure and delete this directory.
