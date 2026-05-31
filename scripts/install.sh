#!/bin/sh
set -e

REPO="AWeirdDev/serverlessd"
BIN_NAME="serverlessd"
INSTALL_DIR="/usr/local/bin"

if [ -t 1 ]; then
  RED="\033[0;31m"
  GREEN="\033[0;32m"
  BOLD="\033[1m"
  RESET="\033[0m"
else
  RED="" GREEN="" BOLD="" RESET=""
fi

error() { printf "${RED}error${RESET}: %s\n" "$*" >&2; }
info()  { printf "${BOLD}info${RESET}: %s\n" "$*"; }
done_() { printf "${GREEN}done${RESET}: %s\n" "$*"; }

# Detect OS
OS="$(uname -s)"
case "$OS" in
  Linux*)  PLATFORM="linux" ;;
  Darwin*) PLATFORM="macos" ;;
  *)
    error "unsupported operating system: $OS"
    exit 1
    ;;
esac

ARCH="$(uname -m)"
case "$ARCH" in
  x86_64|amd64) ARCH="x86_64" ;;
  aarch64|arm64) ARCH="aarch64" ;;
  *)
    error "unsupported architecture: $ARCH"
    exit 1
    ;;
esac

BINARY="${BIN_NAME}-${PLATFORM}-${ARCH}"

info "detected platform: ${PLATFORM}-${ARCH}"
info "Fetching latest release from github.com/${REPO}..."

LATEST_URL="https://api.github.com/repos/${REPO}/releases/latest"
TAG=$(curl -fsSL "$LATEST_URL" | grep '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')

if [ -z "$TAG" ]; then
  error "failed to determine latest release tag"
  exit 1
fi

info "found latest release: $TAG"

DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${TAG}/${BINARY}"

info "downloading ${BINARY}..."
TMP_FILE="$(mktemp)"
curl -fsSL "$DOWNLOAD_URL" -o "$TMP_FILE" || {
  error "failed to download; the binary for ${PLATFORM}-${ARCH} may not be available for release ${TAG}"
  rm -f "$TMP_FILE"
  exit 1
}

chmod +x "$TMP_FILE"

info "installing to ${INSTALL_DIR}/${BIN_NAME}..."
if [ -w "$INSTALL_DIR" ]; then
  mv "$TMP_FILE" "${INSTALL_DIR}/${BIN_NAME}"
else
  printf "\n"
  info "sudo is required for this operation"
  sudo mv "$TMP_FILE" "${INSTALL_DIR}/${BIN_NAME}"
fi

done_  "serverlessd (${TAG}) installed successfully! run 'serverlessd --help' to get started."
