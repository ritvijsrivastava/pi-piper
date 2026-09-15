#!/bin/sh
# Installs the pi-piper Hub binary from GitHub Releases — no Rust toolchain
# needed. The binary is statically linked (musl) and self-contained (the PWA
# is embedded via rust-embed), so a single file is all there is to install.
#
#   curl -fsSL https://raw.githubusercontent.com/ritvijsrivastava/pi-piper/main/scripts/install.sh | sh
#
# Environment overrides:
#   PI_PIPER_INSTALL_VERSION  release tag to install ("latest" by default)
#   PI_PIPER_INSTALL_DIR      install directory (~/.local/bin by default)
set -eu

REPO="ritvijsrivastava/pi-piper"
VERSION="${PI_PIPER_INSTALL_VERSION:-latest}"
INSTALL_DIR="${PI_PIPER_INSTALL_DIR:-$HOME/.local/bin}"

# --- platform detection -----------------------------------------------------

os=$(uname -s)
if [ "$os" != "Linux" ]; then
    echo "error: prebuilt binaries are Linux-only; on this OS use:" >&2
    echo "  cargo install pi-piper" >&2
    exit 1
fi

case "$(uname -m)" in
    x86_64 | amd64) target="x86_64-unknown-linux-musl" ;;
    aarch64 | arm64) target="aarch64-unknown-linux-musl" ;;
    *)
        echo "error: unsupported architecture '$(uname -m)' (prebuilt: x86_64, aarch64)." >&2
        echo "  cargo install pi-piper builds from source on any target." >&2
        exit 1
        ;;
esac

# --- download + verify ------------------------------------------------------

url="https://github.com/$REPO/releases/${VERSION}/download/pi-piper-${target}.tar.gz"
checksum_url="${url}.sha256"
echo "downloading pi-piper $VERSION for $target..."

tmpdir=$(mktemp -d)
trap 'rm -rf "$tmpdir"' EXIT

if command -v curl >/dev/null 2>&1; then
    curl -fsSL --retry 3 -o "$tmpdir/pi-piper.tar.gz" "$url"
    curl -fsSL --retry 3 -o "$tmpdir/pi-piper.tar.gz.sha256" "$checksum_url"
elif command -v wget >/dev/null 2>&1; then
    wget -qO "$tmpdir/pi-piper.tar.gz" "$url"
    wget -qO "$tmpdir/pi-piper.tar.gz.sha256" "$checksum_url"
else
    echo "error: need curl or wget to download the release" >&2
    exit 1
fi

# The checksum file lists "<sha>  pi-piper-<target>.tar.gz"; rename to match
# so `sha256sum -c` can find the file it refers to.
mv "$tmpdir/pi-piper.tar.gz" "$tmpdir/pi-piper-$target.tar.gz"
(cd "$tmpdir" && sha256sum -c "pi-piper.tar.gz.sha256" >/dev/null)
echo "checksum verified."

tar -xzf "$tmpdir/pi-piper-$target.tar.gz" -C "$tmpdir"

# --- install ----------------------------------------------------------------

mkdir -p "$INSTALL_DIR"
mv "$tmpdir/pi-piper-$target/pi-piper" "$INSTALL_DIR/pi-piper"
chmod +x "$INSTALL_DIR/pi-piper"
echo "installed $(basename "$("$INSTALL_DIR/pi-piper" --version)" 2>/dev/null || echo 'pi-piper') to $INSTALL_DIR/pi-piper"

case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
        echo "note: $INSTALL_DIR is not on your PATH — add it, e.g." >&2
        echo "  export PATH=\"$INSTALL_DIR:\$PATH\"" >&2
        ;;
esac
echo "next: run 'pi-piper', then '/rc' in any pi session."
