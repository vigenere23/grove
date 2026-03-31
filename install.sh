#!/usr/bin/env bash
set -euo pipefail

INSTALL_DIR="$HOME/.local/bin"
BINARY_NAME="grove"
PROFILE="${1:-debug}"

echo "Building grove with profile '$PROFILE'..."
case "$PROFILE" in
    release) cargo build --release ;;
    debug)   cargo build ;;
    *)       cargo build --profile "$PROFILE" ;;
esac

BINARY_PATH="$(cd "$(dirname "$0")" && pwd)/target/$PROFILE/$BINARY_NAME"

if [ ! -f "$BINARY_PATH" ]; then
    echo "Error: build failed — binary not found at $BINARY_PATH" >&2
    exit 1
fi

mkdir -p "$INSTALL_DIR"

ln -sf "$BINARY_PATH" "$INSTALL_DIR/$BINARY_NAME"
echo "Symlinked $INSTALL_DIR/$BINARY_NAME -> $BINARY_PATH"

case ":$PATH:" in
    *":$INSTALL_DIR:"*)
        echo "Done. Run 'grove --help' to get started."
        ;;
    *)
        echo ""
        echo "WARNING: $INSTALL_DIR is not in your PATH."
        echo "Add the following line to your shell profile (~/.bashrc, ~/.zshrc, etc.):"
        echo ""
        echo "    export PATH=\"\$HOME/.local/bin:\$PATH\""
        echo ""
        echo "Then restart your shell or run: source ~/.zshrc"
        ;;
esac
