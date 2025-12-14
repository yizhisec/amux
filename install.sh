#!/bin/sh
# amux installer
# Usage: curl -fsSL https://raw.githubusercontent.com/yizhisec/amux/main/install.sh | sh

set -e

REPO="yizhisec/amux"
INSTALL_DIR="${AMUX_INSTALL_DIR:-$HOME/.local/bin}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

info() {
    printf "${GREEN}info${NC}: %s\n" "$1"
}

warn() {
    printf "${YELLOW}warn${NC}: %s\n" "$1"
}

error() {
    printf "${RED}error${NC}: %s\n" "$1" >&2
    exit 1
}

# Detect OS
detect_os() {
    case "$(uname -s)" in
        Linux*)  echo "linux" ;;
        Darwin*) echo "darwin" ;;
        *)       error "Unsupported operating system: $(uname -s)" ;;
    esac
}

# Detect architecture
detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64)  echo "x86_64" ;;
        aarch64|arm64) echo "aarch64" ;;
        *)             error "Unsupported architecture: $(uname -m)" ;;
    esac
}

# Get the target triple
get_target() {
    local os="$1"
    local arch="$2"

    case "$os" in
        linux)  echo "${arch}-unknown-linux-gnu" ;;
        darwin) echo "${arch}-apple-darwin" ;;
    esac
}

# Get latest release version
get_latest_version() {
    curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | \
        grep '"tag_name":' | \
        sed -E 's/.*"([^"]+)".*/\1/'
}

# Download and extract
download_and_install() {
    local version="$1"
    local target="$2"
    local url="https://github.com/${REPO}/releases/download/${version}/amux-${target}.tar.gz"
    local tmp_dir

    tmp_dir=$(mktemp -d)
    trap "rm -rf $tmp_dir" EXIT

    info "Downloading amux ${version} for ${target}..."
    curl -fsSL "$url" -o "$tmp_dir/amux.tar.gz" || error "Failed to download from $url"

    info "Extracting..."
    tar -xzf "$tmp_dir/amux.tar.gz" -C "$tmp_dir"

    info "Installing to ${INSTALL_DIR}..."
    mkdir -p "$INSTALL_DIR"

    # Install binaries
    install -m 755 "$tmp_dir/amux" "$INSTALL_DIR/amux"
    install -m 755 "$tmp_dir/amux-daemon" "$INSTALL_DIR/amux-daemon"

    info "Successfully installed amux ${version}"
}

# Check if INSTALL_DIR is in PATH
check_path() {
    case ":$PATH:" in
        *":$INSTALL_DIR:"*) return 0 ;;
        *) return 1 ;;
    esac
}

main() {
    info "Detecting platform..."
    local os=$(detect_os)
    local arch=$(detect_arch)
    local target=$(get_target "$os" "$arch")

    info "Platform: $os $arch ($target)"

    info "Fetching latest version..."
    local version=$(get_latest_version)

    if [ -z "$version" ]; then
        error "Failed to get latest version. Please check your network connection."
    fi

    info "Latest version: $version"

    download_and_install "$version" "$target"

    # Check PATH
    if ! check_path; then
        echo ""
        warn "Add the following to your shell config (~/.bashrc, ~/.zshrc, etc.):"
        echo ""
        echo "    export PATH=\"\$HOME/.local/bin:\$PATH\""
        echo ""
    fi

    echo ""
    info "Run 'amux --help' to get started!"
}

main "$@"
