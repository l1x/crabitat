# Crabitat — Agent Guidelines

## Package Manager

**Always use `bun` / `bunx`.** Never use `npm`, `npx`, `yarn`, or `pnpm`.

- Install dependencies: `bun install`
- Run scripts: `bun run <script>` or `bunx --bun <command>`
- Dev server: `bunx --bun astro dev`
- Build: `bunx --bun astro build`

## Toolchain

- **Rust**: managed via mise (`mise install`)
- **Bun**: managed via mise
- **Task runner**: mise (see `mise.toml` for all tasks)

## Conventions

- Validate before committing: `mise run verify` (runs fmt + clippy + test)
- Format Rust: `mise run fmt`
- Lint Rust: `mise run clippy`
- Run tests: `mise run test`
