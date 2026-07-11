<p align="center">
  <img src="ui/src/assets/logo.svg" width="80" height="80" alt="Covername">
</p>

<h1 align="center">Covername</h1>

<p align="center">
  <strong>Local-first document anonymization.</strong><br>
  Detect PII. Replace with cover identities. Everything stays on your machine.
</p>

<p align="center">
  <a href="#installation">Install</a> ·
  <a href="#quick-start">Quick Start</a> ·
  <a href="#desktop-app">Desktop App</a> ·
  <a href="docs/design.md">Design</a>
</p>

---

## Features

- **Local-first**: All processing happens on your machine. Document content is never sent anywhere.
- **Consistent replacements**: "Jason Smith" always becomes "John Adams" across all your documents.
- **Smart detection**: Regex patterns (SSN, phone, email, credit cards) + NER-based name/address detection.
- **Interactive review**: Accept, edit, or reject each detection before output is generated.
- **Multiple formats**: Text files, PDFs, Excel spreadsheets (.xlsx), and images (via OCR).
- **OCR support**: Extract text from scanned documents and images using Tesseract.
- **Batch processing**: Process entire directories with consistent identities.
- **Custom rules**: Add your own detection patterns without rebuilding.
- **Export/Import**: Back up and restore your configuration and mappings.

## Installation

### Desktop App (macOS)

Download the latest `.dmg` from [Releases](https://github.com/nycjay/covername/releases), open it, and drag Covername to Applications. No other tools required.

> **First launch**: macOS will warn about an unidentified developer. Right-click → Open → click "Open". You only need to do this once.

### CLI (via Homebrew)

```bash
brew tap nycjay/tap
brew install covername
```

### Build from source

Requires [Rust](https://rustup.rs/) (1.85+), [just](https://github.com/casey/just), and optionally Tesseract for OCR.

```bash
git clone https://github.com/nycjay/covername.git
cd covername
just setup       # Check/install prerequisites
cargo install --path crates/covername-cli
```

For the desktop app: `just tauri-build` (also requires Node.js).

## Quick Start

```bash
# Scan a file for PII (read-only, doesn't modify anything)
covername scan document.txt

# Process a file: detect → review → generate clean output
covername process document.txt

# Process an entire directory (batch mode)
covername process ~/Documents/finances/ --recursive

# Process without interactive review (uses existing mappings, auto-generates new ones)
covername process ~/Documents/finances/ --recursive --auto-accept

# Manage your identity mappings
covername mappings list
covername mappings add --original "Jason Smith" --replacement "John Adams" --type PERSON

# Back up your configuration
covername config export --output backup.zip
```

## How It Works

1. **Scan**: Covername analyzes your document for PII using regex patterns, a dictionary-based NER detector, and optionally an ONNX model or Smart Detection (local AI).
2. **Review**: Each detection is shown with its context. You choose to accept the suggested replacement, edit it, or reject it.
3. **Output**: A new file is generated with all accepted replacements applied. For PDFs, position-aware redaction preserves the original layout — only PII regions are modified. The original file is never touched.

Replacements are remembered. The next time "Jason Smith" appears in any document, Covername automatically uses the same cover name.

## Commands

| Command | Description |
|---------|-------------|
| `covername scan <path> [-r]` | Detect PII without modifying anything |
| `covername process <path> [-r] [--auto-accept]` | Full pipeline: detect → review → output |
| `covername config show\|set\|path\|export\|import` | Manage configuration |
| `covername mappings list\|add\|remove` | Manage identity mappings |
| `covername rules list\|add\|remove\|test` | Manage detection rules |
| `covername ignore list\|add\|remove\|clear` | Manage permanently ignored entities |
| `covername model status\|download\|remove` | NER model management |
| `covername smart-detection status\|download\|remove` | Smart Detection (local AI) |

## Desktop App

A native macOS desktop app — no terminal required. Drag and drop documents, review detections visually, and generate clean output with one click.

- Drag-and-drop files or folders for single or batch processing
- Visual PII highlighting with accept/reject/edit controls
- Batch mode: process an entire folder at once with consistent identities
- Real-time progress during scanning and PDF generation
- Auto-updates from GitHub Releases

See [Installation](#installation) for download and first-launch instructions.

## Smart Detection

Smart Detection is an optional feature that uses a local AI model (~1 GB) to automatically classify detections as personal or corporate/public — reducing false positives without manual review.

```bash
covername smart-detection status    # Check if installed
covername smart-detection download  # Download model (~1 GB, one-time)
covername smart-detection remove    # Remove model files
```

When installed, scans automatically filter out corporate addresses, company phone numbers, and other non-personal detections. Everything runs locally — no data is sent anywhere.

## Configuration

Config lives at `~/.config/covername/`. Key settings:

- `output_pattern`: Output file naming (default: `{name}-covered.{ext}`)
- `output_directory`: Where to write output (default: same as input)
- `model_update_check`: Check for NER model updates on launch

## Uninstall

**Desktop App**: Open Help → About → "Uninstall Covername…". Choose whether to keep downloaded models (saves re-downloading later) or remove everything.

**CLI** (Homebrew): `brew uninstall covername`

**Manual cleanup**: Remove `~/.config/covername/` to delete all config, mappings, and models.

## Development

```bash
just check        # Format check + lint + test (CI equivalent)
just setup        # Verify/install prerequisites (PDFium, Tesseract)
just build        # Compile workspace
just test         # Run tests
just lint         # Clippy with -D warnings
just fmt          # Auto-format
just run          # Run the CLI (e.g., just run scan file.txt)
just ui-install   # Install frontend dependencies
just ui-build     # Build the Svelte UI
just tauri-dev    # Run Tauri desktop app in dev mode
just tauri-build  # Build .app and .dmg
just release      # Full release build
```

### Feature flags

```bash
cargo build -p covername-cli --features onnx              # ONNX NER model
cargo build -p covername-cli --features smart-detection   # Local AI classification
```

## Project Status

Phase 1 (MVP) — complete:
- [x] Text file processing
- [x] PDF processing (text extraction + clean PDF output)
- [x] Regex-based PII detection (SSN, phone, email, credit card, account numbers)
- [x] Dictionary-based NER (person names, addresses)
- [x] Interactive CLI review with batch accept
- [x] Persistent identity mappings
- [x] Custom detection rules
- [x] Batch/recursive processing
- [x] Config export/import

Phase 2 (OCR + formats) — complete:
- [x] XLSX processing (read, detect PII, write clean spreadsheet with cell-level replacement)
- [x] OCR for scanned documents and images (via command-line Tesseract)
- [x] Image file support (.png, .jpg, .jpeg, .tiff, .tif)
- [x] PDF OCR fallback (scanned PDFs with little/no extractable text)
- [x] Position-aware PDF redaction (hOCR bounding boxes, visual layout preserved)
- [x] ONNX-based NER model for improved detection (feature-gated)
- [x] Smart Detection: local AI classification (feature-gated)

Phase 3 (Desktop app) — complete:
- [x] Tauri v2 + Svelte 5 desktop app
- [x] Document viewer with PII highlighting
- [x] Accept/reject/edit workflow with sidebar
- [x] Progress bars during scanning and PDF generation
- [x] Branded UI with logo, welcome screen, help dialog
- [x] macOS .app and .dmg builds

Remaining:
- [ ] MCP server for AI agent integration

## AI Agent Integration

Covername can be used by AI agents (Claude, ChatGPT, Kiro, etc.) to anonymize documents before analysis. Use `--auto-accept` for non-interactive processing and `--json` for structured output:

```bash
# Scan and get structured results
covername --json scan document.pdf

# Process without interactive review, structured output
covername --json --quiet process document.pdf --auto-accept

# Quiet mode: suppress all informational output, only errors
covername --quiet process ~/Documents/ --recursive --auto-accept
```

See [docs/AGENT-SKILL.md](docs/AGENT-SKILL.md) for the full agent skill specification, including typical workflows, supported PII types, and configuration.

## License

MIT — see [LICENSE](LICENSE).
