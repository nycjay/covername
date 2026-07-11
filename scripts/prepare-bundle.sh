#!/bin/bash
# Prepare native binaries for bundling inside the Tauri .app.
# Run this before `just tauri-build` if the binaries aren't present.
#
# Requires: brew install tesseract
# Downloads: PDFium from GitHub releases

set -eu

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TAURI_DIR="$SCRIPT_DIR/../crates/covername-tauri"

echo "=== Preparing native binaries for Covername.app ==="

mkdir -p "$TAURI_DIR/lib" "$TAURI_DIR/bin" "$TAURI_DIR/tessdata"

# --- PDFium ---
if [ ! -f "$TAURI_DIR/lib/libpdfium.dylib" ]; then
    echo "Downloading PDFium..."
    PDFIUM_VERSION="chromium/7920"
    PDFIUM_URL="https://github.com/bblanchon/pdfium-binaries/releases/download/${PDFIUM_VERSION}/pdfium-mac-arm64.tgz"
    curl -sL -o /tmp/pdfium.tgz "$PDFIUM_URL"
    tar -xzf /tmp/pdfium.tgz -C /tmp/pdfium-extract 2>/dev/null || (mkdir -p /tmp/pdfium-extract && tar -xzf /tmp/pdfium.tgz -C /tmp/pdfium-extract)
    cp /tmp/pdfium-extract/lib/libpdfium.dylib "$TAURI_DIR/lib/"
    rm -rf /tmp/pdfium.tgz /tmp/pdfium-extract
    echo "  ✓ PDFium"
else
    echo "  ✓ PDFium (already present)"
fi

# --- Tesseract ---
if [ ! -f "$TAURI_DIR/bin/tesseract" ]; then
    echo "Copying Tesseract from Homebrew..."

    TESS_PREFIX=$(brew --prefix tesseract)
    LEPT_PREFIX=$(brew --prefix leptonica)

    # Binary
    cp "$(realpath "$TESS_PREFIX/bin/tesseract")" "$TAURI_DIR/bin/tesseract"

    # Core libraries
    cp "$(realpath "$TESS_PREFIX/lib/libtesseract.5.dylib")" "$TAURI_DIR/lib/libtesseract.5.dylib"
    cp "$(realpath "$LEPT_PREFIX/lib/libleptonica.6.dylib")" "$TAURI_DIR/lib/libleptonica.6.dylib"

    # Transitive deps
    for lib in libarchive.13 libpng16.16 libjpeg.8 libtiff.6 libwebp.7 libwebpmux.3 libopenjp2.7 libzstd.1 liblzma.5 liblz4.1 libb2.1 libsharpyuv.0; do
        # Find the dylib via otool chain or brew
        found=$(find /opt/homebrew -name "${lib}.dylib" -o -name "${lib}.*.dylib" 2>/dev/null | head -1)
        if [ -n "$found" ]; then
            cp "$(realpath "$found")" "$TAURI_DIR/lib/${lib}.dylib"
        fi
    done

    # Special: giflib doesn't have version in filename
    gif=$(find /opt/homebrew -name "libgif.dylib" 2>/dev/null | head -1)
    [ -n "$gif" ] && cp "$(realpath "$gif")" "$TAURI_DIR/lib/libgif.dylib"

    # Training data
    cp "$TESS_PREFIX/share/tessdata/eng.traineddata" "$TAURI_DIR/tessdata/"

    # Config files (needed for hocr output mode)
    cp -r "$TESS_PREFIX/share/tessdata/configs" "$TAURI_DIR/tessdata/"

    echo "  ✓ Tesseract + dependencies"
else
    echo "  ✓ Tesseract (already present)"
fi

# --- Fix dylib paths ---
echo "Fixing library paths..."

# Rewrite all /opt/homebrew references to @loader_path
for dylib in "$TAURI_DIR"/lib/*.dylib; do
    otool -L "$dylib" 2>/dev/null | grep "/opt/homebrew" | awk '{print $1}' | while read -r dep; do
        install_name_tool -change "$dep" "@loader_path/$(basename "$dep")" "$dylib" 2>/dev/null || true
    done
    install_name_tool -id "@loader_path/$(basename "$dylib")" "$dylib" 2>/dev/null || true
done

# Fix @rpath references
for dylib in "$TAURI_DIR"/lib/*.dylib; do
    otool -L "$dylib" 2>/dev/null | grep "@rpath" | awk '{print $1}' | while read -r dep; do
        install_name_tool -change "$dep" "@loader_path/$(basename "$dep")" "$dylib" 2>/dev/null || true
    done
done

# Fix tesseract binary to find libs in ../lib/
otool -L "$TAURI_DIR/bin/tesseract" | grep "/opt/homebrew" | awk '{print $1}' | while read -r dep; do
    install_name_tool -change "$dep" "@executable_path/../lib/$(basename "$dep")" "$TAURI_DIR/bin/tesseract" 2>/dev/null || true
done

# --- Sign everything ---
echo "Ad-hoc signing..."
chmod u+w "$TAURI_DIR"/lib/*.dylib "$TAURI_DIR/bin/tesseract"
xattr -cr "$TAURI_DIR/lib" "$TAURI_DIR/bin" "$TAURI_DIR/tessdata" 2>/dev/null || true
codesign --sign - --force "$TAURI_DIR/bin/tesseract" "$TAURI_DIR"/lib/*.dylib 2>/dev/null

echo ""
echo "=== Done! Run 'just tauri-build' to create the .app ==="
