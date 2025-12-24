# Crabitat - Agent Development Guidelines

## Commands (use mise tasks)
- Build: `mise run build-dev` / `mise run build-prod`
- Lint: `mise run lint` (fails on warnings)
- Verify: `mise run verify` (lint + tests)
- Test all: `mise run tests` 
- Test single: `cargo test test_name` (e.g., `cargo test test_external_id_creation`)
- Run: `cargo run -- examples/project.toml`
- Coverage: `mise run coverage` (requires cargo-tarpaulin)

## Code Style
- Rust 2024 edition with tokio async runtime
- Use `thiserror` for error handling with `#[from]` conversions
- Serde for all config/data structs with `#[derive(Serialize, Deserialize)]`
- Prefix external IDs: `aid-`, `task-`, `proj-`, `uid-`
- Struct fields: `snake_case`, public for config
- Methods: `snake_case` with self/ownership clarity
- Use `#[cfg(test)]` for test modules
- Import order: std, external crates, local modules
- Use `env_logger` with info level filtering
- Async functions use `?` operator for error propagation