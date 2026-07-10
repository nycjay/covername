# Covername — Project Steering

## Overview

Covername is a local-first document anonymization tool built in Rust. It detects PII in documents and replaces it with consistent cover identities. This is the developer's first Rust project — prioritize clarity, idiomatic Rust, and thorough explanations.

## Language & Tooling

- **Language**: Rust (latest stable edition, 2024)
- **Build system**: Cargo (workspace)
- **Task runner**: just (justfile)
- **Linter**: clippy (pedantic)
- **Formatter**: rustfmt
- **Test framework**: built-in `#[test]` + `#[cfg(test)]` modules
- **CI-ready**: all checks runnable via `just check`

## Rust Conventions

### Code Style

- Use `rustfmt` defaults — do not override `rustfmt.toml` unless there's a strong reason.
- Run `clippy` with `--deny warnings` in CI. In development, treat warnings as things to fix.
- Prefer `clippy::pedantic` for learning best practices, but allow specific lints where pedantic is unhelpful (document these in `Cargo.toml` or `lib.rs`).

### Error Handling

- Use `thiserror` for library error types (in `covername-core`).
- Use `anyhow` for the CLI binary (in `covername-cli`) where rich error context matters more than typed errors.
- Never use `.unwrap()` in library code. Use `.expect("reason")` only where panic is truly impossible.
- Prefer `?` operator for propagation. Write clear error messages that help the user understand what went wrong.

### Naming

- Crate names: `covername-core`, `covername-cli`
- Module names: lowercase, snake_case
- Types: PascalCase
- Functions/methods: snake_case
- Constants: SCREAMING_SNAKE_CASE
- Avoid abbreviations unless universally understood (e.g., `PII`, `NER`, `PDF` are fine)

### Module Organization

- One concept per module. If a module exceeds ~300 lines, consider splitting.
- Use `mod.rs` pattern for module directories.
- Keep `pub` surface area minimal — only expose what the CLI (or future Tauri app) needs.
- Use `pub(crate)` for internal-but-shared items.

### Documentation

- All public functions, types, and modules must have doc comments (`///`).
- Include a brief description and, for non-obvious functions, a usage example.
- Use `//!` module-level docs to explain the purpose of each module.

### Testing

- Unit tests go in the same file, inside `#[cfg(test)] mod tests { ... }`.
- Integration tests go in `tests/` directories within each crate.
- Test fixture files (sample PDFs, text files) go in the workspace-level `test-fixtures/` directory.
- Name tests descriptively: `test_detects_ssn_format`, not `test1`.
- Use `assert_eq!` with clear expected/actual labels where possible.

### Dependencies

- Pin exact versions in `Cargo.toml` (e.g., `serde = "=1.0.210"`).
- Prefer well-maintained crates with recent activity.
- Minimize dependency count — don't add a crate for something achievable in a few lines.
- Audit new dependencies: check downloads, last update, open issues.

### Performance

- Don't optimize prematurely. Correctness and clarity first.
- For Phase 1, processing speed is not critical (documents are small, user is waiting anyway).
- When performance matters later: use `cargo bench` with `criterion` crate.

## Project Structure

```
covername/
├── .kiro/
│   └── steering.md              # This file
├── Cargo.toml                   # Workspace definition
├── justfile                     # Build/test/lint automation
├── rustfmt.toml                 # Formatter config (defaults)
├── docs/
│   ├── design.md                # Architecture & design
│   ├── implementation-plan.md   # Ordered build steps
│   └── AGENT-SKILL.md           # AI agent integration spec
├── ui/                          # Svelte 5 + Vite frontend (for Tauri app)
├── crates/
│   ├── covername-core/          # Library crate
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── config.rs
│   │       ├── detection.rs
│   │       ├── document.rs
│   │       ├── error.rs
│   │       ├── export.rs
│   │       ├── ignore.rs
│   │       ├── mapping.rs
│   │       ├── ner/
│   │       │   ├── mod.rs
│   │       │   ├── dictionary.rs
│   │       │   ├── model_manager.rs
│   │       │   └── onnx.rs
│   │       ├── ocr.rs
│   │       ├── output.rs
│   │       ├── pdf_output.rs
│   │       ├── pdfium.rs
│   │       ├── pipeline.rs
│   │       ├── processor.rs
│   │       ├── redact.rs
│   │       ├── replacement.rs
│   │       ├── smart_detection.rs
│   │       ├── utils.rs
│   │       └── xlsx.rs
│   ├── covername-cli/           # Binary crate
│   └── covername-tauri/         # Tauri v2 desktop app (built separately from workspace)
├── test-fixtures/               # Shared test data
└── .gitignore
```

## Git Conventions

- Branch from `main` for features: `feat/<short-description>`
- Commit messages: imperative mood, concise first line (<70 chars)
  - `Add regex rule engine with SSN and phone patterns`
  - `Fix PDF text extraction for multi-page documents`
- Keep commits atomic — one logical change per commit.
- No personal information in code, comments, or commit messages.

## Justfile Commands

The `justfile` provides these standard commands:

| Command | Purpose |
|---------|---------|
| `just check` | Run fmt-check + lint + test (CI equivalent) |
| `just setup` | Install dependencies and set up dev environment |
| `just build` | Compile all crates |
| `just test` | Run all tests |
| `just lint` | Run clippy with pedantic warnings |
| `just fmt` | Format all code |
| `just fmt-check` | Check formatting without modifying |
| `just run` | Run the CLI binary |
| `just clean` | Clean build artifacts |
| `just build-onnx` | Build with ONNX feature enabled |
| `just ui-install` | Install frontend dependencies |
| `just ui-build` | Build the Svelte frontend |
| `just tauri-dev` | Run Tauri app in development mode |
| `just tauri-build` | Build Tauri app for distribution |
| `just release` | Build release artifacts |

## Tauri Conventions

The desktop app (`crates/covername-tauri/`) follows these patterns:

- **Async commands with `spawn_blocking`**: Tauri commands are async. CPU-bound work (scanning, processing) runs inside `tokio::task::spawn_blocking` to avoid blocking the main thread.
- **Progress events via `app.emit`**: Long-running operations emit progress events to the frontend using `app.emit("progress", payload)`. The frontend subscribes to these for progress bars.
- **Byte-to-char offset conversion**: The Rust core works with byte offsets, but the Svelte frontend needs character offsets for highlighting. Conversion happens in the Tauri command layer before sending data to the UI.
- **Single source of truth**: Tauri commands call `covername-core` pipeline functions — no detection/processing logic in the Tauri crate itself.

## Design Principles

1. **Offline-first**: Document content never leaves the machine. No telemetry, no analytics.
2. **Privacy by design**: No PII in logs, error messages, or debug output.
3. **User-curated**: The tool suggests, the user decides. Mappings are user-owned data.
4. **Composable**: Core library is independent of CLI. Future UIs are thin wrappers.
5. **Fail gracefully**: If a document can't be processed, explain why clearly and continue with the batch.

## Code Quality Principles

### Single Source of Truth

Every piece of logic should exist in exactly ONE place. If both CLI and Tauri need the same behavior, it lives in `covername-core` (usually `pipeline.rs`). The CLI and Tauri are thin wrappers that call core functions.

**Anti-pattern (caused bugs twice):**
```rust
// CLI main.rs
fn extract_text(file: &Path) -> String { /* custom OCR logic */ }

// Tauri main.rs  
fn scan_file(path: String) { /* different OCR logic */ }
```

**Correct:**
```rust
// covername-core/src/pipeline.rs
pub fn extract_text(file: &Path) -> Result<String> { /* single implementation */ }

// CLI: covername_core::pipeline::extract_text(file)
// Tauri: covername_core::pipeline::extract_text(file)
```

### Where Logic Lives

| Layer | Responsibility | Example |
|-------|---------------|----------|
| `pipeline.rs` | Orchestration (extract, detect, filter) | `scan_file()`, `detect_pii()` |
| `processor.rs` | Pure data transforms | `apply_replacements()`, `merge_detections()` |
| Other core modules | Single-purpose implementations | `ocr.rs`, `redact.rs`, `detection.rs` |
| CLI `main.rs` | User interaction only | Prompts, formatting, arg parsing |
| Tauri `main.rs` | IPC bridge only | Serialize/deserialize, call pipeline |

**Rule:** If you find yourself writing detection/extraction/replacement logic in the CLI or Tauri, STOP. It belongs in covername-core.

### No Silent Failures

Never discard errors with `let _ = ...` in production code. At minimum:
```rust
if let Err(e) = potentially_failing_operation() {
    tracing::warn!(error = %e, "operation failed, using fallback");
}
```

### Shared Utilities

When the same logic appears in 2+ places, extract immediately:
- `pdfium.rs` — PDFium library loading
- `utils.rs` — progress_style(), extract_context()
- `Config` methods — storage path resolution

Don't wait for it to cause a bug — extract on the second occurrence.

### Feature Parity

When adding a capability (like OCR support or ONNX detection):
1. Implement in `covername-core` (usually `pipeline.rs`)
2. Both CLI and Tauri automatically get it
3. NEVER add a capability only to one interface

### Testing Boundaries

Core logic (pipeline, processor, detection) should have unit tests. CLI/Tauri should be tested by:
1. Running `just check` (compilation + clippy)
2. Manual integration testing
3. Future: integration tests that invoke the binary

## What to Explain

Since this is a first Rust project, when writing code:

- Explain ownership/borrowing decisions when non-obvious.
- Explain why a particular pattern (e.g., trait objects vs enums, Arc vs Rc) was chosen.
- Link to relevant Rust Book chapters or documentation when introducing new concepts.
- Prefer explicit over clever — readable code that a Rust beginner can follow.
