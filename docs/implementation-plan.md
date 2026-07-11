# Covername — Phase 1 Implementation Plan

This is the ordered step-by-step plan for building the Phase 1 MVP. Each step builds on the previous one. Steps are designed to be individually testable — you should be able to run tests after each step.

---

## Step 0: Project Scaffolding ✅

**Goal**: Empty Rust workspace that compiles, lints, and has automation ready.

- [x] Create workspace `Cargo.toml`
- [x] Create `covername-core` crate (library)
- [x] Create `covername-cli` crate (binary, depends on core)
- [x] Set up `justfile` with build/test/lint/fmt/check commands
- [x] Create `rustfmt.toml` (use defaults)
- [x] Create `.gitignore` (Rust standard + models/ + test output)
- [x] Create basic `README.md` (project name, one-line description, "WIP")
- [x] Verify: `just check` passes (empty project compiles, no warnings)

**Output**: A compiling workspace with tooling. `just check` is green.

---

## Step 1: Configuration & Storage Foundation ✅

**Goal**: The app can read/write config and knows where to store data.

- [x] Define `Config` struct in core (output_pattern, output_directory, model_version, etc.)
- [x] Implement config file read/write (JSON, using serde)
- [x] Implement storage path (`~/.config/covername/`, respects `XDG_CONFIG_HOME`)
- [x] Create storage directory on first access if it doesn't exist
- [x] Add CLI `config show`, `config set`, and `config path` commands
- [x] Write tests: config serialization/deserialization, default values, path resolution

**Output**: `covername config show` prints defaults. `covername config set output_pattern "{name}-covered.{ext}"` persists.

---

## Step 2: Replacement Mapping Store ✅

**Goal**: Persistent storage of entity-to-replacement mappings.

- [x] Define `Mapping` struct (original, replacement, entity_type, created, last_used)
- [x] Define `MappingStore` that loads/saves mappings.json
- [x] Implement CRUD operations (add, remove, list, lookup by original)
- [x] Add CLI `mappings list`, `mappings add`, `mappings remove` commands
- [x] Write tests: add/remove/lookup, persistence round-trip, duplicate handling

**Output**: `covername mappings add --original "Jason Smith" --replacement "John Adams" --type PERSON` works end-to-end.

---

## Step 3: Regex Rule Engine ✅

**Goal**: Detect PII using configurable regex patterns.

- [x] Define `Rule` struct (name, pattern, entity_type, enabled)
- [x] Define `RuleEngine` that loads rules and runs them against text
- [x] Implement built-in default rules: SSN, phone, email, credit card, account number patterns
- [x] Implement `Detection` struct (matched_text, entity_type, start_pos, end_pos, context)
- [x] Add CLI `rules list`, `rules add`, `rules remove`, `rules test` commands
- [x] Write tests: each default pattern with positive/negative cases, custom rule matching

**Output**: `covername rules test --pattern "\d{3}-\d{2}-\d{4}" --input "SSN: 123-45-6789"` finds the match.

---

## Step 4: Plain Text Processing Pipeline ✅

**Goal**: Full scan → detect → review → output cycle for .txt files.

- [x] Implement text file reader (TextDocument)
- [x] Wire detection (rule engine) to produce detections from a text document
- [x] Implement deterministic replacement generator (name lists, format-preserving for numbers)
- [x] Implement interactive CLI review flow (show detection, accept/edit/reject/batch-accept)
- [x] Implement text output writer (apply replacements, write new file)
- [x] Write tests: full pipeline with fixture file, verify output has no original PII

**Output**: `covername process sample.txt` → interactive review → produces `sample-covered.txt`.

---

## Step 5: NER Model Integration ✅

**Goal**: Add ML-based entity detection alongside regex rules.

- [x] Define `NerDetector` trait with detect/name/is_ready methods
- [x] Implement `DictionaryDetector` (heuristic name/address detection, no model file needed)
- [x] Implement `OnnxDetector` (feature-gated, full ONNX inference pipeline)
- [x] Implement model download/status/remove infrastructure (ModelManager)
- [x] Merge NER detections with regex detections (deduplicate overlaps)
- [x] Add CLI `model status`, `model download`, `model remove` commands
- [x] Write tests: dictionary detector, BIO label decoding, detection merging

**Output**: `covername scan sample.txt` detects names/addresses that regex alone would miss.

**Note**: ONNX model support is behind `--features onnx` flag. Default build uses dictionary detector. To use the real model: `covername model download` then build with `cargo build --features onnx`.

---

## Step 6: PDF Text Extraction ✅

**Goal**: Extract text from text-based PDFs.

- [x] Integrate `pdf-extract` crate for PDF reading
- [x] Implement `PdfDocument` with `from_file()` and `extract_text()`
- [x] Implement `detect_file_type()` for routing .txt vs .pdf
- [x] Handle multi-page PDFs
- [x] Write tests: extract text from generated PDFs, verify content matches

**Output**: `covername scan document.pdf` produces detections from PDF content.

---

## Step 7: PDF Output with True Redaction ✅

**Goal**: Produce a new PDF with PII replaced, ensuring original text is truly removed.

- [x] Implement `write_pdf()` using `printpdf` (generates fresh PDF from replaced text)
- [x] Handle line wrapping and pagination
- [x] Verify redaction: output PDF bytes do not contain original PII strings
- [x] Update output dispatcher to route .pdf files to PDF writer
- [x] Write tests: round-trip pipeline, binary search for PII in output

**Output**: `covername process document.pdf` → `document-covered.pdf` with no original PII.

**Note**: Phase 1 output is plain-text-in-PDF (loses original formatting/layout). Preserving visual fidelity is a Phase 2+ goal.

---

## Step 8: Config Export/Import ✅

**Goal**: Users can back up and restore their configuration, mappings, and rules.

- [x] Implement export: bundle config.json + mappings.json + custom-rules.json into a ZIP
- [x] Implement import: extract ZIP, validate contents, overwrite existing
- [x] Add CLI `config export --output backup.zip` and `config import --input backup.zip`
- [x] Write tests: export → import round-trip, invalid ZIP handling

**Output**: `covername config export --output backup.zip` → `covername config import --input backup.zip`.

---

## Step 9: Batch Processing ✅

**Goal**: Process multiple files in one command with consistent mappings.

- [x] Implement `collect_files()` for directory traversal (recursive optional)
- [x] Accept multiple file paths or a directory in `scan` and `process`
- [x] Apply same mappings across all files in a batch
- [x] Show batch summary after processing
- [x] Handle partial failures gracefully (one file fails, others continue)
- [x] Add `--recursive` flag for directory scanning
- [x] Write tests: batch with mixed file types, consistent mapping application

**Output**: `covername process ~/Documents/finances/ --recursive` processes all supported files.

---

## Step 10: Polish & First Release ✅

**Goal**: Ready for personal daily use.

- [x] Comprehensive error messages for common failures
- [x] `--help` text with long descriptions for scan/process
- [x] `--version` flag (0.1.0)
- [x] README with installation instructions, usage examples, and feature overview
- [x] Create synthetic test fixtures
- [x] Run full `just check` — all tests pass, no warnings, formatted

**Output**: A usable `covername` binary you can `cargo install` and use daily.

---

## Dependencies Between Steps

```
Step 0 (scaffold) ✅
  ├── Step 1 (config) ✅
  │     └── Step 2 (mappings) ✅
  │           └── Step 3 (rules) ✅
  │                 └── Step 4 (text pipeline) ✅
  │                       ├── Step 5 (NER model) ✅
  │                       ├── Step 6 (PDF extraction) ✅
  │                       │     └── Step 7 (PDF output) ✅
  │                       └── Step 9 (batch) ✅
  └── Step 8 (export/import) ✅

Step 10 (polish) ✅
```

## Final Stats (as of Phase 2 completion)

| Metric | Value |
|--------|-------|
| Tests (total) | 148 (139 unit + 9 integration) |
| Ignored tests (require Tesseract) | 1 |
| Clippy warnings | 0 |
| File formats supported | .txt, .md, .csv, .pdf, .xlsx, .xls, .png, .jpg, .jpeg, .tiff, .tif |
| Detection types | SSN, phone, email, credit card, account number, person name, address, date, credentials |
| CLI commands | 8 top-level (scan, process, config, mappings, rules, ignore, model, smart-detection) |
| Feature flags | `onnx` (ML inference), `smart-detection` (llama.cpp), `download` (model fetching, default) |

---

## What's Next (Phases 3-5)

### Phase 3: Tauri + Svelte Desktop App ✅ COMPLETE
- [x] Tauri v2 app scaffolding with Svelte 5 frontend
- [x] Document viewer with PDF rendering + PII highlight overlays
- [x] Accept/reject sidebar UI
- [x] Drag-and-drop file selection
- [x] Settings UI for rules, mappings, config
- [x] macOS `.dmg` distribution
- [ ] Tauri Updater plugin for auto-updates
- [ ] Explore Open Design for UI prototyping

### Phase 4: Polish & Distribution
- Model auto-update with user prompt
- macOS code signing and notarization
- Homebrew formula
- Performance optimization for large documents
- Contribution guidelines for open-source

### Phase 5: MCP Server (Consumer AI Integration)
- `crates/covername-mcp/` — MCP protocol server
- Tools: `anonymize_file`, `scan_file`, `list_mappings`, `add_mapping`
- Integration with Claude Desktop, ChatGPT, and other MCP clients
- Stdio server (launched on demand, no background process)

### Ongoing Improvements
- [ ] [x] Upgraded ONNX model to ettin-68m-nemotron-pii (96% F1, 55 entity types)
- [ ] PDF output fidelity (preserve layout/fonts instead of plain-text-in-PDF)
- [ ] More regex rules (date-of-birth, passport, driver's license patterns)
- [ ] Real-world testing with actual financial/health documents
- [x] License decision: MIT

### SLM Integration (Smart Detection)
- [ ] **Context-aware PII classification** — use a small local language model to:
  - Classify detections as "personal PII" vs "corporate/public info"
  - Auto-accept high-confidence personal PII, only prompt for ambiguous ones
  - Reduce user review burden (from 4 items to 0-1 on typical documents)
- [ ] **Model candidates**: Phi-3 Mini (3.8B), Qwen2.5 (1.5B), or similar via ONNX/llama.cpp
  - ~1-2GB model file, loads only during processing
  - Runs entirely locally (same privacy guarantee)
- [ ] **App setup flow**:
  - First launch: "Install Smart Detection? (downloads ~1-2GB model)" [Yes / Skip]
  - Detect if a compatible model is already installed (e.g., from Ollama)
  - Can be enabled/disabled in Settings
  - Optional — tool works fine without it (heuristics + NER model)
- [ ] **Generate better replacements** — SLM suggests contextually-appropriate fake names
  (matching gender/ethnicity patterns of the original)

### Uninstall / Cleanup
- [ ] **Uninstall models** — `covername model remove-all` or Settings → "Remove downloaded models"
  - Removes NER model (~262MB)
  - Removes SLM model (~1-2GB) when added
  - Keeps config/mappings/ignore list intact
- [x] **Full uninstall** — Help → About → "Uninstall Covername…"
  - Option A: Keep models, remove app + config/mappings/rules/logs
  - Option B: Remove everything (~1 GB including models)
  - Moves .app to Trash via Finder (user can undo)
  - Confirmation step with clear description of what will be removed
- [x] **Storage usage display** — Settings → "Storage" shows disk usage:
  - Config/mappings: X KB
  - NER model: ~262 MB
  - SLM model: 1.5 GB
  - Logs: X MB
  - Total: X.X GB

### Infrastructure TODO
- [x] **Structured logging** — use `tracing` crate, log to `~/.config/covername/logs/`
  - Dev mode: log to terminal + file
  - Release mode: log to file only
  - Log rotation (keep last 7 days or 50MB)
  - No PII in logs (only entity types, positions, file names)
- [ ] **"Gather debug logs" feature** — menu option in Tauri app that:
  - Locates log files at `~/.config/covername/logs/`
  - Zips them up (redacting any accidental PII)
  - Saves to Desktop or opens share dialog
  - For CLI: `covername debug-logs --output debug-bundle.zip`
- [ ] **Bundle PDFium in .app** — include `libpdfium.dylib` in `Contents/Frameworks/`
  so users don't need a separate download
- [ ] **Bundle Tesseract in .app** — include tesseract binary + eng.traineddata
  in `Contents/Resources/` for fully self-contained distribution
- [ ] **Auto-download PDFium in just setup** — currently downloads on first run,
  document the `PDFIUM_DYNAMIC_LIB_PATH` env var requirement
- [x] **Progress bar during processing** — show progress for:
  - PDF page rendering (per page)
  - OCR (per page)
  - Batch processing (per file)
  - Model download
  - CLI: use `indicatif` crate (already a dependency)
  - Tauri: send progress events to frontend via Tauri events API
