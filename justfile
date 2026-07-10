# List available commands
default:
    just --list

# Compile all crates
build:
    cargo build

# Run all tests
test:
    cargo test

# Run clippy with pedantic warnings
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Format all code
fmt:
    cargo fmt --all

# Check formatting without modifying
fmt-check:
    cargo fmt --all -- --check

# Run fmt-check + lint + test (CI equivalent)
check: fmt-check lint test

# Run the CLI binary
run *ARGS:
    cargo run -p covername-cli -- {{ARGS}}

# Clean build artifacts
clean:
    cargo clean

# Check and install prerequisites
setup:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "Checking Covername prerequisites..."
    echo ""

    MISSING=0

    # Rust
    if command -v cargo &>/dev/null; then
        echo "  ✓ Rust ($(rustc --version | cut -d' ' -f2))"
    else
        echo "  ✗ Rust — install from https://rustup.rs"
        MISSING=1
    fi

    # Node.js (for UI)
    if command -v node &>/dev/null; then
        echo "  ✓ Node.js ($(node --version))"
    else
        echo "  ✗ Node.js — install from https://nodejs.org or: brew install node"
        MISSING=1
    fi

    # Tauri CLI
    if command -v npx &>/dev/null && npx tauri --version &>/dev/null 2>&1; then
        echo "  ✓ Tauri CLI ($(npx tauri --version))"
    else
        echo "  ✗ Tauri CLI — install with: npm install -g @tauri-apps/cli"
        MISSING=1
    fi

    # Tesseract (OCR)
    if command -v tesseract &>/dev/null; then
        echo "  ✓ Tesseract OCR ($(tesseract --version 2>&1 | head -1))"
    else
        echo "  ✗ Tesseract OCR — install with: brew install tesseract"
        MISSING=1
    fi

    # PDFium (PDF rendering - linked as library, check if binary available)
    if [ -f "${PDFIUM_DYNAMIC_LIB_PATH:-$HOME/lib/pdfium/lib}/libpdfium.dylib" ]; then
        echo "  ✓ PDFium library (found)"
    else
        echo "  ✗ PDFium library — downloading now..."
        mkdir -p "$HOME/lib/pdfium"
        # Pinned to a specific release for reproducibility
        PDFIUM_VERSION="chromium/7920"
        PDFIUM_URL="https://github.com/bblanchon/pdfium-binaries/releases/download/${PDFIUM_VERSION}/pdfium-mac-arm64.tgz"
        curl -sL -o /tmp/pdfium.tgz "$PDFIUM_URL"
        # Verify download succeeded and is a valid archive
        if file /tmp/pdfium.tgz | grep -q "gzip"; then
            tar -xzf /tmp/pdfium.tgz -C "$HOME/lib/pdfium"
            rm -f /tmp/pdfium.tgz
        else
            echo "  ✗ PDFium download failed or was corrupted"
            rm -f /tmp/pdfium.tgz
            MISSING=1
        fi
        if [ -f "$HOME/lib/pdfium/lib/libpdfium.dylib" ]; then
            # Ad-hoc sign so hardened macOS apps can load it
            codesign --sign - --force "$HOME/lib/pdfium/lib/libpdfium.dylib" 2>/dev/null
            echo "  ✓ PDFium library (installed to ~/lib/pdfium/lib)"
            echo ""
            echo "  Add to your shell profile:"
            echo "    export PDFIUM_DYNAMIC_LIB_PATH=\$HOME/lib/pdfium/lib"
        else
            echo "  ✗ PDFium download failed. Manually download from:"
            echo "    https://github.com/bblanchon/pdfium-binaries/releases"
            MISSING=1
        fi
    fi

    # just (task runner)
    if command -v just &>/dev/null; then
        echo "  ✓ just ($(just --version))"
    else
        echo "  ✗ just — install with: brew install just"
        MISSING=1
    fi

    echo ""
    if [ $MISSING -eq 0 ]; then
        echo "All prerequisites installed! ✓"
        echo ""
        echo "Quick start:"
        echo "  just build        # compile the CLI"
        echo "  just ui-install   # install UI dependencies"
        echo "  just tauri-dev    # launch the desktop app"
    else
        echo "Some prerequisites are missing. Install them with:"
        echo "  brew install tesseract just"
        echo "  npm install -g @tauri-apps/cli"
        echo ""
        echo "Then run 'just setup' again to verify."
    fi

# Build with ONNX NER model support
build-onnx:
    cargo build --features onnx

# Install UI dependencies
ui-install:
    cd ui && npm install

# Build the UI (for Tauri production build)
ui-build:
    cd ui && npm run build

# Start the Tauri desktop app in dev mode
tauri-dev:
    cd ui && npm run dev &
    cd crates/covername-tauri && npx tauri dev

# Build the Tauri desktop app for distribution
tauri-build: ui-build
    cd crates/covername-tauri && npx tauri build

# Create a release: validate version, tag, and optionally push
# Usage: just release 0.2.0
release VERSION:
    #!/usr/bin/env bash
    set -euo pipefail

    NEW_VERSION="{{VERSION}}"

    # Validate version format (semver)
    if ! echo "$NEW_VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+$'; then
        echo "Error: Version must be semver (e.g., 0.2.0), got: $NEW_VERSION"
        exit 1
    fi

    # Get current version from Cargo.toml
    CURRENT_VERSION=$(grep '^version' crates/covername-core/Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
    echo "Current version: $CURRENT_VERSION"
    echo "New version:     $NEW_VERSION"

    # Validate version goes up
    if [ "$(printf '%s\n' "$CURRENT_VERSION" "$NEW_VERSION" | sort -V | tail -1)" = "$CURRENT_VERSION" ]; then
        echo "Error: New version ($NEW_VERSION) must be greater than current ($CURRENT_VERSION)"
        exit 1
    fi

    # Check for uncommitted changes
    if ! git diff --quiet || ! git diff --cached --quiet; then
        echo "Error: You have uncommitted changes. Commit or stash them first."
        exit 1
    fi

    # Check we're on main
    BRANCH=$(git branch --show-current)
    if [ "$BRANCH" != "main" ]; then
        echo "Warning: You're on branch '$BRANCH', not 'main'. Continue? (y/n)"
        read -r CONFIRM
        if [ "$CONFIRM" != "y" ]; then
            echo "Aborted."
            exit 1
        fi
    fi

    # Update versions in all Cargo.toml files
    echo "Updating Cargo.toml versions..."
    sed -i '' "s/^version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/" crates/covername-core/Cargo.toml
    sed -i '' "s/^version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/" crates/covername-cli/Cargo.toml
    sed -i '' "s/^version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/" crates/covername-tauri/Cargo.toml

    # Update tauri.conf.json version
    sed -i '' "s/\"version\": \"$CURRENT_VERSION\"/\"version\": \"$NEW_VERSION\"/" crates/covername-tauri/tauri.conf.json

    # Update Cargo.lock
    cargo check --quiet 2>/dev/null || true

    # Commit and tag
    git add -A
    git commit -m "Release v$NEW_VERSION"
    git tag "v$NEW_VERSION"

    echo ""
    echo "✓ Version bumped to $NEW_VERSION"
    echo "✓ Committed: 'Release v$NEW_VERSION'"
    echo "✓ Tagged: v$NEW_VERSION"
    echo ""
    echo "To publish: git push origin main --tags"
