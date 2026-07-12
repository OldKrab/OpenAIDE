# Contributing to OpenAIDE

OpenAIDE is an alpha project. Contributions are welcome, but storage formats,
protocols, and user-facing behavior are still evolving.

## Before you start

- Read `CONTEXT.md`, `PRODUCT.md`, `DESIGN.md`, and the ADRs relevant to your change.
- Search existing issues and pull requests before proposing duplicate work.
- For security vulnerabilities, follow `SECURITY.md` instead of opening a public issue.

## Development workflow

1. Fork the repository and create a focused feature branch.
2. Install Node.js 24, npm, and the stable Rust toolchain.
3. Run `npm ci`.
4. Make the smallest cohesive change with tests at the user-visible, protocol, or
   storage boundary.
5. Run `npm run ci` before opening a pull request.

Protocol changes must regenerate TypeScript bindings with
`npm run protocol:generate`; verify them with `npm run protocol:check`.

Do not commit credentials, personal domains, real home-directory paths, local
runtime state, diagnostics, screenshots, or machine-specific configuration.
Use `example.com`, loopback addresses, and generic fixture users in tests.

## Pull requests

Describe the problem, the chosen behavior, and the verification performed. Keep
unrelated refactors separate. Pull requests must pass all required GitHub checks
and receive review before merge.

By contributing, you agree that your contribution is licensed under the
repository's AGPL-3.0-only license.
