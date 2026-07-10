# Covername — AI Agent Skill

## Purpose

Covername is a local CLI tool that detects and replaces personally identifiable information (PII) in documents. Use it to anonymize documents before analyzing them, so sensitive data never reaches external services.

## When to Use

Use Covername when the user:
- Wants to share a document containing PII with you for analysis
- Asks for financial, medical, or legal advice based on personal documents
- Needs to upload documents to external services safely
- Wants to remove names, SSNs, account numbers, phone numbers, emails, or addresses from files

## Prerequisites

- `covername` CLI must be installed: `cargo install --path crates/covername-cli` from the repo
- Working directory awareness: know where the user's documents are located

## Commands

### Scan (read-only detection)

```bash
covername scan <file_or_directory> [-r]
```

Shows detected PII without modifying anything. Use this to preview what will be anonymized.

### Process (anonymize with auto-accept)

```bash
covername process <file> --auto-accept
```

Detects PII, automatically accepts all suggested replacements, and generates a clean output file. The output file is named `<original>-covered.<ext>` by default.

For directories:
```bash
covername process <directory> --recursive --auto-accept
```

### Structured output (--json)

```bash
covername --json scan <file>
covername --json process <file> --auto-accept
covername --json mappings list
```

Returns machine-parseable JSON. Ideal for agent integration — parse the output directly without regex.

### Quiet mode (--quiet)

```bash
covername --quiet process <file> --auto-accept
```

Suppresses progress bars and informational output. Only errors (stderr) and results are emitted. Combine with `--json` for clean structured output with no noise.

### Check mappings

```bash
covername mappings list
```

Shows all stored identity mappings. Useful for understanding what names map to what replacements.

### Add a specific mapping

```bash
covername mappings add --original "Real Name" --replacement "Cover Name" --type PERSON
```

Pre-load a mapping before processing, so a specific replacement is used.

### Ignore a detection

```bash
covername ignore add "Company Name"
covername ignore list
covername ignore remove "Company Name"
```

Mark specific text as non-PII so it is skipped in future scans.

## Typical Agent Workflow

### Step 1: Anonymize the document

```bash
covername process /path/to/bank-statement.pdf --auto-accept
```

Output: `/path/to/bank-statement-covered.pdf`

### Step 2: Work with the anonymized version

Read and analyze `bank-statement-covered.pdf` instead of the original. All names, account numbers, SSNs etc. are replaced with consistent cover identities.

**Note**: If Smart Detection is enabled (`covername smart-detection status`), the tool uses a local LLM to classify detections as personal vs corporate/public, filtering out false positives before replacement — making `--auto-accept` safer and more accurate.

### Step 3: Map results back (if needed)

```bash
covername mappings list
```

This shows the mapping table. When referring to entities in your response, you can mentally map "John Adams" (cover name) back to the real identity for the user.

## Supported File Types

| Extension | Support |
|-----------|---------|
| `.txt` | Full (detect + replace) |
| `.md` | Full (detect + replace) |
| `.csv` | Full (detect + replace) |
| `.pdf` | Full (text extraction + new PDF output) |
| `.xlsx`, `.xls` | Full (cell-level replacement) |
| `.png`, `.jpg`, `.tiff` | OCR (requires tesseract installed) |

## PII Types Detected

- **PERSON** — Names (first, last, full names)
- **ADDRESS** — Street addresses, cities, states, zip codes
- **SSN** — Social Security Numbers (XXX-XX-XXXX)
- **PHONE** — US phone numbers in various formats
- **EMAIL** — Email addresses
- **CREDIT_CARD** — Visa, Mastercard, Amex, Discover
- **ACCOUNT_NUMBER** — Bank/financial account numbers

## Key Behaviors

- **Never modifies the original file** — always creates a new `-covered` version
- **Consistent replacements** — "Jason Smith" always becomes the same cover name across all documents
- **Remembers mappings** — previously assigned cover names are reused automatically
- **Offline** — no network calls during processing; all detection is local

## Configuration

Config is stored at `~/.config/covername/`. Key settings:

```bash
# Change output file naming pattern
covername config set output_pattern "{name}-anonymized.{ext}"

# Set a dedicated output directory
covername config set output_directory /tmp/anonymized

# View current config
covername config show
```

## Error Handling

- If a file type is unsupported, covername prints a clear error
- If tesseract isn't installed and an image/scanned PDF is provided, it suggests `brew install tesseract`
- If no PII is detected, it prints "No PII detected" and generates no output

## Example Session

```bash
$ covername process ~/Documents/tax-return-2024.pdf --auto-accept
Auto-accepted 12 detection(s).

Summary:
  Detections: 12
  Accepted:   12
  Output:     /Users/user/Documents/tax-return-2024-covered.pdf

$ covername mappings list
ORIGINAL                       REPLACEMENT                    TYPE            LAST USED
------------------------------------------------------------------------------------------
Jason Smith                    Benjamin Harrison              PERSON          2026-07-05 18:04
Jane Smith                     Martha Washington              PERSON          2026-07-05 18:04
123-45-6789                    900-12-4752                    SSN             2026-07-05 18:04
(555) 867-5309                 (555) 555-4390                 PHONE           2026-07-05 18:04
```

## Smart Detection (Optional)

If the user has Smart Detection enabled, Covername automatically classifies detections as personal vs corporate/public — reducing what needs review.

```bash
# Check if Smart Detection is installed
covername smart-detection status

# If not installed and user wants better accuracy:
covername smart-detection download   # ~1 GB, one-time
```

When Smart Detection is active, `--auto-accept` becomes safer since corporate/public false positives are already filtered out before acceptance.
