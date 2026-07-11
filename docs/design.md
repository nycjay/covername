# Covername — Design Document

A local-first document anonymization tool that detects and replaces personally identifiable information (PII) in documents before they are shared with external services like ChatGPT.

## Problem Statement

Users want to upload personal financial documents (bank statements, tax forms, health records) to public LLMs for advice, but these documents contain sensitive PII — names, addresses, account numbers, SSNs. There is no simple, offline tool for non-technical users to strip this information while preserving document structure and consistency across multiple documents.

## Goals

- Detect PII in PDFs (text and scanned), Excel, and plain text files
- Replace PII with consistent, deterministic alternatives across documents
- Produce output documents that look as close to the original as possible
- Run entirely locally — never send document content to a remote server
- Be easy to install, use, and uninstall for non-technical users on macOS
- Remember replacement mappings and user-defined detection rules over time
- Support batch processing with batch accept/reject

## Non-Goals (for now)

- Windows/Linux support (future consideration)
- Real-time monitoring or always-on background processes
- Integration with specific LLM services
- Collaborative/multi-user features

---

## Architecture

```
┌─────────────────────────────────────────────┐
│          Tauri Desktop App (Phase 3+)       │
│   ┌─────────────────────────────────────┐   │
│   │  Web UI (document viewer, sidebar)  │   │
│   └──────────────────┬──────────────────┘   │
│                      │ Tauri commands       │
├──────────────────────┼──────────────────────┤
│          CLI Binary (Phase 1-2)             │
│                      │                      │
├──────────────────────┴──────────────────────┤
│          Core Library (Rust crate)          │
│   ┌──────────┬───────────┬──────────────┐   │
│   │Detection │Replacement│ Document I/O │   │
│   │(NER +    │(mappings, │ (PDF, XLSX,  │   │
│   │ rules)   │ storage)  │  text, OCR)  │   │
│   └──────────┴───────────┴──────────────┘   │
│   ┌──────────┬───────────┐                  │
│   │  Config  │   Model   │                  │
│   │& Storage │ Management│                  │
│   └──────────┴───────────┘                  │
└─────────────────────────────────────────────┘
```

The core library is a Rust crate (`covername-core`) that contains all logic. Both the CLI and Tauri app are thin wrappers that call into this crate. Key orchestration lives in `pipeline.rs` (single source of truth for processing). An ignore list (`ignore.rs`) allows users to mark specific detections as non-PII so they are skipped in future scans.

---

## Tech Stack

### Language: Rust

Chosen for:
- Single binary distribution (no runtime dependencies for the user)
- Memory safety without garbage collection
- Excellent cross-platform support for future Windows/Linux
- Strong ecosystem for the libraries we need
- Tauri is Rust-native

### Key Crates

| Purpose | Crate | Notes |
|---------|-------|-------|
| CLI framework | `clap` | Standard Rust CLI parsing with subcommands |
| PDF reading | `pdf-extract` + `lopdf` | Text extraction from PDF |
| PDF rendering | `pdfium-render` | Page rasterization for OCR and redaction |
| PDF writing/manipulation | `lopdf` | Rebuild PDFs with replacements |
| XLSX read/write | `calamine` (read) + `rust_xlsxwriter` (write) | Preserve spreadsheet structure |
| OCR | command-line `tesseract` | External binary, requires `brew install tesseract` |
| NER model inference | `ort` (ONNX Runtime) | Run NER model without Python (feature-gated) |
| Smart Detection | `llama-cpp-v3` | Local LLM for context-aware PII classification |
| Regex/pattern matching | `regex` | User-defined detection rules |
| Configuration | `serde` + `serde_json` | Config and mappings persistence |
| Image processing | `imageproc` + `ab_glyph` | Redaction overlay and text rendering |
| Progress bars | `indicatif` | CLI progress display |
| Logging | `tracing` | Structured logging |
| HTTP (model downloads) | `reqwest` | Model download and update checks |
| Async runtime | `tokio` | For model downloads and batch processing |
| Tauri (Phase 3) | `tauri` | Desktop app framework |

### NER Model

- **Model**: A pre-trained NER model exported to ONNX format (ettin-68m-nemotron-pii, 96% F1, MIT license)
- **Size**: ~100-500MB on disk
- **Runtime**: ONNX Runtime (`ort` crate) — loads model into memory only during processing
- **Entities detected**: PERSON, ADDRESS, PHONE, EMAIL, SSN, ACCOUNT_NUMBER, DATE_OF_BIRTH, ORGANIZATION (when contextually PII)
- **Supplemented by**: Regex rules for structured patterns (SSN format, credit card numbers, etc.)

### OCR

- **Engine**: Tesseract 5.x via command-line binary (NOT Rust bindings)
- **Install**: Requires `brew install tesseract` (not bundled with the app)
- **Integration**: Invokes `tesseract` CLI with hOCR output for position-aware text extraction
- **Graceful fallback**: If tesseract is not installed, OCR-dependent features are skipped with a helpful message

---

## Data Flow

### Processing Pipeline

```
Input File(s)
     │
     ▼
┌─────────────┐
│ File Type   │ Determine handler: PDF, XLSX, TXT, image
│ Detection   │
└──────┬──────┘
       │
       ▼
┌─────────────┐
│ Text        │ PDF → extract text layers + positions
│ Extraction  │ Scanned PDF/Image → OCR → text + positions
│             │ XLSX → cell contents + positions
│             │ TXT → raw text
└──────┬──────┘
       │
       ▼
┌─────────────┐
│ PII         │ 1. Run NER model on extracted text
│ Detection   │ 2. Run regex/pattern rules
│             │ 3. Check existing mappings (known entities)
│             │ Output: list of (entity, type, position, context)
└──────┬──────┘
       │
       ▼
┌─────────────┐
│ Replacement │ For each detected entity:
│ Resolution  │ - If mapping exists → use stored replacement
│             │ - If new → generate deterministic suggestion
│             │ Output: list of (original, replacement, type, position)
└──────┬──────┘
       │
       ▼
┌─────────────┐
│ User Review │ Present detections to user (CLI prompts)
│             │ Accept / Edit / Reject each, or batch accept
│             │ Highlight missed PII → create new rule
│             │ Save confirmed mappings to storage
└──────┬──────┘
       │
       ▼
┌─────────────┐
│ Document    │ PDF → redact original text, render replacement
│ Rebuild     │      (flatten/rasterize redacted regions)
│             │ XLSX → replace cell contents, write new file
│             │ TXT → string replacement, write new file
│             │ Ensure no original PII remains in output
└──────┬──────┘
       │
       ▼
Output File(s)
  <name>-covered.<ext>  (configurable pattern)
```

### Critical Security Requirement

For PDF output, PII must be **actually removed**, not just visually covered. The approach:

1. For text-based PDFs: Remove the original text object from the PDF structure and insert new text at the same position
2. For scanned/image PDFs: Rasterize the page (or redacted regions), paint over the PII pixels, render replacement text as a new image layer
3. Verify: The output file's raw bytes must not contain the original PII strings

---

## Storage & Configuration

### Storage Location

```
~/.config/covername/
├── config.json              # App settings
├── mappings.json            # Entity replacement mappings
├── rules/
│   └── custom-rules.json   # User-defined detection rules
├── models/
│   ├── manifest.json        # Available model versions
│   └── ner-v1.0/           # Downloaded NER model files
└── history/
    └── sessions.json        # Processing history (which files, when)
```

### config.json

```json
{
  "output_pattern": "{name}-covered.{ext}",
  "output_directory": null,
  "model_version": "ner-v1.0",
  "model_update_check": true,
  "update_check_url": "https://github.com/<org>/covername/releases/latest/manifest.json",
  "ocr_language": "eng"
}
```

- `output_directory`: `null` means same folder as input. Can be set to a specific path.
- `output_pattern`: Supports `{name}`, `{ext}`, `{date}` placeholders.

### mappings.json

```json
{
  "version": 1,
  "entities": [
    {
      "original": "Jason Smith",
      "replacement": "John Adams",
      "type": "PERSON",
      "created": "2026-07-05T12:00:00Z",
      "last_used": "2026-07-05T12:30:00Z"
    },
    {
      "original": "4521-8834-2211",
      "replacement": "9999-0000-1111",
      "type": "ACCOUNT_NUMBER",
      "created": "2026-07-05T12:00:00Z",
      "last_used": "2026-07-05T12:30:00Z"
    }
  ]
}
```

### custom-rules.json

```json
{
  "version": 1,
  "rules": [
    {
      "name": "Health Member ID",
      "description": "Matches member IDs from health provider statements",
      "pattern": "(?:Member\\s*ID|Member\\s*#)\\s*:?\\s*(\\w{6,12})",
      "entity_type": "ACCOUNT_NUMBER",
      "enabled": true,
      "created": "2026-07-05T12:00:00Z"
    }
  ]
}
```

### Export/Import

The entire `~/.config/covername/` directory (minus the models) can be exported as a single JSON or ZIP file and imported on another machine or after reinstall.

---

## CLI Interface

### Commands

```bash
# Core workflow
covername scan <file_or_directory> [--recursive]    # Detect PII in file(s)
covername process <file_or_directory> [--auto]       # Scan + review + output in one step

# Mapping management
covername mappings list                              # Show all stored mappings
covername mappings add --original "X" --replacement "Y" --type PERSON
covername mappings remove --original "X"
covername mappings export --output mappings.json
covername mappings import --input mappings.json

# Rule management
covername rules list
covername rules add --name "Member ID" --pattern "(?:Member\s*ID)\s*:?\s*(\w+)" --type ACCOUNT_NUMBER
covername rules remove --name "Member ID"
covername rules test --pattern "..." --input sample.txt    # Test a rule against a file

# Model management
covername model status                               # Show installed model + available updates
covername model update                               # Download latest model
covername model check                                # Check for updates without installing

# Configuration
covername config show
covername config set output_pattern "{name}-covered.{ext}"
covername config set output_directory ~/Documents/covered
covername config export --output config-backup.zip   # Full config + mappings
covername config import --input config-backup.zip
```

### Batch Accept in Review

During the review flow, in addition to per-item accept/reject:

```
Review options:
  (y) Accept this    (n) Reject this    (e) Edit replacement
  (a) Accept ALL remaining             (A) Accept all of same type
  (q) Quit (save progress)
```

---

## Deterministic Replacement Generation

When a new entity is detected and no mapping exists, the tool suggests a replacement:

- **Names**: Draw from a fixed list of historical/fictional names, selected by hashing the original name. Same original always gets the same suggestion. List is large enough (~1000 names) to avoid collisions.
- **Addresses**: Generate a plausible but fake address preserving the structure (number, street, city, state, zip) using seeded random generation from the original.
- **Account numbers**: Preserve format/length, replace digits deterministically (e.g., hash-based).
- **SSNs**: Replace with a number in the 900-999 range (IRS-designated non-valid range).
- **Phone numbers**: Replace with 555-xxxx numbers (reserved for fictional use).
- **Emails**: Replace with `<replacement_name>@example.com`.

The user can always override the suggestion. Once confirmed, the mapping is stored permanently.

---

## MVP Scope (Phase 1) ✅ COMPLETE

### In Scope

- [x] Project scaffolding (Rust workspace with `covername-core` and `covername-cli` crates)
- [x] Text extraction from text-based PDFs
- [x] PII detection via regex rules (SSN, phone, email, credit card, account number patterns)
- [x] PII detection via NER model (PERSON, ADDRESS, ORGANIZATION)
- [x] Replacement mapping storage (create, persist, reuse)
- [x] CLI review flow with batch accept
- [x] Plain text file output with replacements applied
- [x] PDF output with text replacement (text-based PDFs only)
- [x] Configuration management (output pattern, output directory)
- [x] Model download on first run
- [x] Custom rule creation via CLI
- [x] Export/import of config and mappings

### Out of Scope for Phase 1 (done in Phase 2)

- ~~Scanned PDF / image OCR~~ ✅ Done
- ~~XLSX processing~~ ✅ Done
- Tauri desktop UI (Phase 3)
- Document render view with highlights (Phase 3)
- Model auto-update checking
- Windows/Linux builds

---

## Phased Delivery Plan

### Phase 1: Core Engine + CLI (MVP) ✅ COMPLETE

**Goal**: Process text-based PDFs and plain text files via CLI.

- [x] Rust workspace setup
- [x] Text extraction from PDFs
- [x] NER model integration (ONNX Runtime + dictionary detector)
- [x] Regex rule engine (SSN, phone, email, credit card, account number)
- [x] Replacement mapping store
- [x] CLI scan/review/process workflow
- [x] PDF output (text replacement with proper redaction)
- [x] Plain text output
- [x] Config and mapping export/import
- [x] Unit and integration tests with test fixtures
- [x] `--auto-accept` flag for non-interactive use
- [x] Agent skill file (docs/AGENT-SKILL.md)
- [x] GitHub Actions CI

### Phase 2: OCR + More Formats ✅ COMPLETE

**Goal**: Handle scanned documents and spreadsheets.

- [x] OCR integration via command-line Tesseract (runtime check, graceful fallback)
- [x] Image file support (PNG, JPEG, TIFF)
- [x] PDF OCR fallback (if text extraction yields <50 chars, tries OCR)
- [x] XLSX read (calamine) + write (rust_xlsxwriter) with cell-level replacement
- [x] Real ONNX model download from HuggingFace (kalyan-ks/ettin-68m-nemotron-pii)

### Phase 3: Desktop App (Tauri) ✅ COMPLETE

**Goal**: Wrap the core in a native desktop app.

- [x] Tauri v2 app scaffolding (`crates/covername-tauri/`)
- [x] Svelte 5 frontend (in `ui/` directory, built with Vite)
- [x] Document viewer with PDF rendering
- [x] PII highlight overlay (Option B UI from design discussion)
- [x] Sidebar with detection list and accept/reject controls
- [x] Drag-and-drop file selection (single file or folder)
- [x] Batch processing: drop a folder or multiple files, process all at once
- [x] Progress bars (scanning, per-page PDF redaction, per-file batch)
- [x] Auto-updates via Tauri updater plugin (checks GitHub Releases)
- [x] Settings UI (rules, mappings, config)
- [x] macOS `.dmg` distribution with branded background

**UI Design Tool**: Explore [Open Design](https://github.com/nexu-io/open-design) for generating/iterating on the UI design. Open Design is an AI-native design platform with design system support — may help with rapid prototyping of the document viewer and review sidebar UI.

### Phase 4: Polish & Distribution

**Goal**: Production-ready for potential open-source release.

- Model auto-update with user prompt
- Batch processing progress UI
- Undo/reprocess workflow
- Performance optimization for large documents
- macOS code signing and notarization
- Documentation and README
- Contribution guidelines if open-sourcing

### Phase 5: MCP Server (Consumer AI Integration)

**Goal**: Let consumer AI tools (Claude Desktop, ChatGPT, etc.) use Covername automatically.

Users who aren't developers shouldn't need to use the CLI. An MCP (Model Context Protocol) server allows AI desktop apps to call Covername on behalf of the user — when they drag a file into chat, the AI anonymizes it transparently.

**New crate**: `crates/covername-mcp/`

**MCP tools to expose:**
- `anonymize_file(path)` → processes file with auto-accept, returns path to anonymized output
- `scan_file(path)` → returns detected PII list (type, matched text, context) without modifying
- `list_mappings()` → returns all stored identity mappings
- `add_mapping(original, replacement, type)` → pre-set a specific replacement

**Installation for Claude Desktop:**
```json
// ~/.config/claude/claude_desktop_config.json
{
  "mcpServers": {
    "covername": {
      "command": "covername-mcp",
      "args": []
    }
  }
}
```

**User experience:**
1. User installs `covername-mcp` once (e.g., `brew install covername`)
2. Adds it to Claude Desktop config (one-time setup)
3. From then on, when they share documents containing PII, Claude can call `anonymize_file` before analyzing content

**Implementation notes:**
- Use `rmcp` or similar Rust MCP crate for the protocol layer
- The MCP server wraps `covername-core` — same logic as CLI, different interface
- Runs as a stdio server (launched by Claude Desktop on demand, not always running)
- No additional background processes needed

---

## Project Structure

```
covername/
├── Cargo.toml                    # Workspace root
├── justfile                      # Build/test/lint automation
├── rustfmt.toml                  # Formatter config
├── .gitignore
├── README.md
├── .github/
│   └── workflows/ci.yml          # GitHub Actions CI
├── .kiro/
│   └── steering.md               # Project conventions & Rust best practices
├── docs/
│   ├── design.md                 # This document
│   ├── implementation-plan.md    # Ordered build steps (Phase 1)
│   └── AGENT-SKILL.md            # AI agent integration spec
├── ui/                           # Svelte 5 + Vite frontend (for Tauri app)
├── crates/
│   ├── covername-core/           # Core library
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs            # Public API
│   │   │   ├── config.rs         # Configuration management
│   │   │   ├── detection.rs      # Regex rule engine + Rule/Detection types
│   │   │   ├── document.rs       # File type detection, text/PDF/XLSX reading
│   │   │   ├── error.rs          # Error types (thiserror)
│   │   │   ├── export.rs         # Config export/import (ZIP)
│   │   │   ├── ignore.rs         # Ignore list management
│   │   │   ├── mapping.rs        # Replacement mapping store
│   │   │   ├── ner/
│   │   │   │   ├── mod.rs        # NerDetector trait
│   │   │   │   ├── dictionary.rs # Heuristic name/address detection
│   │   │   │   ├── model_manager.rs # Model download/status/remove
│   │   │   │   └── onnx.rs       # ONNX model inference (feature-gated)
│   │   │   ├── ocr.rs            # OCR via command-line Tesseract (hOCR output)
│   │   │   ├── output.rs         # Output path resolution + file writing
│   │   │   ├── pdf_output.rs     # PDF generation
│   │   │   ├── pdfium.rs         # PDFium library loading
│   │   │   ├── pipeline.rs       # Processing orchestration (single source of truth)
│   │   │   ├── processor.rs      # Detection resolution + replacement application
│   │   │   ├── redact.rs         # Position-aware PDF redaction (hOCR bounding boxes)
│   │   │   ├── replacement.rs    # Deterministic fake data generation
│   │   │   ├── smart_detection.rs # LLM-based context-aware PII classification
│   │   │   ├── utils.rs          # Shared utilities (progress_style, extract_context)
│   │   │   └── xlsx.rs           # XLSX read (calamine) + write (rust_xlsxwriter)
│   │   └── tests/
│   │       ├── pipeline_test.rs  # Integration tests
│   │       └── pdf_pipeline_test.rs
│   ├── covername-cli/            # CLI binary
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── main.rs           # Clap CLI with all subcommands
│   └── covername-tauri/          # Tauri v2 desktop app (built separately from workspace)
│       ├── Cargo.toml
│       ├── tauri.conf.json
│       └── src/
│           └── main.rs           # Tauri commands (scan_file, generate_output)
├── test-fixtures/
│   └── sample.txt                # Synthetic bank statement with PII
└── .gitignore
```

---

## Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Language | Rust | Single binary, no runtime deps, fast, memory-safe, Tauri-native |
| NER runtime | ONNX Runtime | Run ML models without Python; models are portable, updatable independently |
| CLI first | Yes | Testable, validates core logic before investing in UI |
| PDF redaction | Flatten/rasterize redacted regions | Ensures PII is truly removed, not just hidden |
| Replacement strategy | Deterministic (hash-based) + user override | Consistent suggestions without randomness; user has final say |
| Storage format | JSON files | Human-readable, easy to debug, export/import trivially |
| Model distribution | Separate download | Keeps install size small; model updates don't require app updates |
| Offline-first | Required | Document content never leaves the machine |
| Output format | Same as input | Preserves document utility (PDF→PDF, XLSX→XLSX) |

---

## Security Considerations

- **No network during processing**: Document content is never transmitted. Network is used only for model update checks and downloads.
- **True redaction**: Output files must not contain original PII in any layer (text, metadata, hidden objects).
- **Local storage**: All mappings, rules, and config stored in user-local application support directory with standard macOS file permissions.
- **No PII in logs**: Processing logs must not contain detected PII values, only entity types and positions.
- **Export safety**: Config exports contain mappings (which include original PII values) — user should be warned about this.

---

## Open Questions

1. **~~Which pre-trained NER model?~~** DECIDED: Using `kalyan-ks/ettin-68m-nemotron-pii` (~416MB, BERT-base). See NER Model Selection below.
2. **~~PDF library choice~~** DECIDED: Using `pdf-extract` + `lopdf` for reading, `pdfium-render` for rasterization, and position-aware redaction via hOCR bounding boxes in `redact.rs`. Position-aware PDF redaction IS implemented.
3. **~~Font handling in PDF output~~** RESOLVED: Using `imageproc` + `ab_glyph` for text rendering on rasterized pages.
4. **Scanned PDF quality**: OCR accuracy varies greatly with scan quality. May need preprocessing (deskew, contrast adjustment) before OCR.

---

## NER Model Selection

### Current: `kalyan-ks/ettin-68m-nemotron-pii`

- **Source**: https://huggingface.co/kalyan-ks/ettin-68m-nemotron-pii
- **Size**: ~262MB (ONNX model) + 3.4MB (tokenizer)
- **Base**: Ettin-encoder-68m (ModernBERT architecture), fine-tuned on nvidia/Nemotron-PII
- **Labels**: 107 (55 entity types × BIO + O)
- **Entity types**: first_name, last_name, street_address, city, state, postcode, ssn, phone_number, email, account_number, credit_debit_card, date_of_birth, company_name, and 42 more
- **F1 score**: 96.27% (beats GPT-4o-mini at PII extraction)
- **License**: MIT
- **Download**: Hosted on GitHub Releases (model-ettin-68m-pii-v1 tag)
- **Why switched from barflyman/bert-pii-detect-onnx**: The ettin model is smaller (262MB vs 430MB), significantly more accurate (96% vs unknown), detects 55 entity types (vs ~10), and is MIT licensed.

### Detection strategy (three layers)

| Layer | What it catches | Always available? |
|-------|----------------|-------------------|
| **Regex rules** | SSN, phone, email, credit card, account numbers | Yes (built-in) |
| **Dictionary NER** | Person names (capitalized word heuristic), addresses | Yes (no model needed) |
| **ONNX model** | Names, addresses, dates, credentials, IDs, organizations, 55+ types | Only with `--features onnx` + model downloaded |

### Alternatives evaluated

| Model | Size | F1 | Best for | Link |
|-------|------|-----|----------|------|
| **kalyan-ks/ettin-68m-nemotron-pii** | 262MB | 96% | General PII (CURRENT) | https://huggingface.co/kalyan-ks/ettin-68m-nemotron-pii |
| **kalyan-ks/ettin-17m-nemotron-pii** | ~70MB | 94% | Lightweight/mobile | https://huggingface.co/kalyan-ks/ettin-17m-nemotron-pii |
| **StanfordAIMI/stanford-deidentifier-base** | ~500MB | — | Medical/clinical (HIPAA) | https://huggingface.co/StanfordAIMI/stanford-deidentifier-base |
| **dslim/bert-base-NER** | ~262MB | General entity recognition, strong PERSON detection | https://huggingface.co/dslim/bert-base-NER |
| **lakshyakh93/deberta_finetuned_pii** | ~800MB | Kaggle PII competition winner, high accuracy | https://huggingface.co/lakshyakh93/deberta_finetuned_pii |

### Switching models

The `NerDetector` trait abstracts the model choice. To switch:
1. Export the new model to ONNX format
2. Update the download URL in `ModelManager`
3. Adjust tokenizer and label mapping in `OnnxDetector`
4. Run test suite to verify detection quality

Users can switch by: `covername model remove` → `covername model download`

---

## Update Strategy (Phase 3+)

Covername has two independent update channels:

### 1. App Updates (Tauri Updater Plugin)

When we build the Tauri desktop app, use the [Tauri Updater v2 plugin](https://v2.tauri.app/plugin/updater/) for binary updates:

- Endpoint: GitHub Releases (`/releases/latest/download/update.json`)
- Requires code signing (Tauri generates a keypair)
- On launch, checks for new app version → prompts user → downloads and restarts
- User config/mappings are preserved across updates (stored in `~/.config/covername/`)

### Versioning Strategy

Versions are derived from **git tags**:

- Tags follow semver: `v0.1.0`, `v0.2.0`, `v1.0.0`
- Pushing a tag triggers the release GitHub Action
- The action builds `.dmg` + `update.json` and publishes to GitHub Releases
- `Cargo.toml` version is the source of truth; tag must match
- In dev mode, the window title shows: `Covername (Dev) — v0.1.0 (abc1234)`
- In release mode: `Covername v0.1.0`

**Release flow:**
```bash
# Bump version in Cargo.toml files
# Commit: "Release v0.2.0"
git tag v0.2.0
git push origin main --tags
# → GitHub Action builds .dmg, publishes release, generates update.json
# → Users with the app installed get prompted to update
```

### 2. Model Updates (Custom ModelManager)

The NER model is updated independently of the app:

- Endpoint: separate manifest.json in GitHub Releases (or a dedicated models repo)
- On launch (if `model_update_check` is true), checks manifest for newer version
- Prompts user: "NER model v1.1 available. Download now?" (optional, not forced)
- Downloads to `~/.config/covername/models/` — no app restart needed
- Old model kept until new one is verified

### Design decisions to revisit in Phase 3:

- [ ] Generate Tauri updater signing keys and store securely
- [ ] Decide on update frequency check (every launch? daily? weekly?)
- [ ] Decide if model updates should be fully automatic or always prompt
- [ ] Set up GitHub Actions to publish `update.json` alongside releases
- [ ] Consider whether CLI-only users need an update check (probably not — they use `brew upgrade` or similar)

---

## Internationalization (i18n) — Future

The app should be architected to support multiple languages even though the initial release is English-only:

- All user-facing strings externalized via a string key system (e.g., `t('detection.accept')`)
- Date/time/number formatting respects locale
- RTL layout support in CSS (flexbox + logical properties)
- Locale detection from system settings
- Translation files in JSON (one per language)
- Candidate languages for first expansion: Spanish, French, German, Japanese

**Implementation**: Use a lightweight i18n library in the Svelte frontend (e.g., `svelte-i18n` or `paraglide-js`). The CLI can use simple string tables initially.
