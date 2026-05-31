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

fetch_latest_tag() {
  LATEST_URL="https://api.github.com/repos/${REPO}/releases/latest"
  TAG=$(curl -fsSL "$LATEST_URL" | grep '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')
  if [ -z "$TAG" ]; then
    error "failed to determine latest release tag"
    exit 1
  fi
  info "found latest release: $TAG"
}

download_and_install() {
  local remote_name="$1"
  local install_name="$2"

  info "downloading ${remote_name}..."
  TMP_FILE="$(mktemp)"
  DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${TAG}/${remote_name}"
  curl -fsSL "$DOWNLOAD_URL" -o "$TMP_FILE" || {
    error "failed to download; '${remote_name}' may not be available for release ${TAG}"
    rm -f "$TMP_FILE"
    exit 1
  }

  chmod +x "$TMP_FILE"

  info "installing to ${INSTALL_DIR}/${install_name}..."
  if [ -w "$INSTALL_DIR" ]; then
    mv "$TMP_FILE" "${INSTALL_DIR}/${install_name}"
  else
    printf "\n"
    info "sudo is required for this operation"
    sudo mv "$TMP_FILE" "${INSTALL_DIR}/${install_name}"
  fi
}

cmd_install() {
  local BINARY="${BIN_NAME}-${PLATFORM}-${ARCH}"
  info "detected platform: ${PLATFORM}-${ARCH}"
  info "fetching latest release from github.com/${REPO}..."
  fetch_latest_tag
  download_and_install "$BINARY" "$BIN_NAME"
  done_ "serverlessd (${TAG}) installed successfully! run 'serverlessd --help' to get started."
}

cmd_add() {
  local BINDING_NAME="$1"
  if [ -z "$BINDING_NAME" ]; then
    error "usage: install.sh add <binding-name>"
    exit 1
  fi

  local REMOTE_NAME="binding-${BINDING_NAME}-${PLATFORM}-${ARCH}"
  local INSTALL_NAME="serverlessd-binding-${BINDING_NAME}"

  info "detected platform: ${PLATFORM}-${ARCH}"
  info "fetching latest release from github.com/${REPO}..."
  fetch_latest_tag
  download_and_install "$REMOTE_NAME" "$INSTALL_NAME"
  done_ "binding '${BINDING_NAME}' (${TAG}) installed successfully!"
}

# --- entrypoint ---

case "${1:-}" in
  "")
    cmd_install
    ;;
  add)
    shift
    cmd_add "$@"
    ;;
  *)
    error "unknown command: $1"
    printf "usage:\n"
    printf "  install.sh            install serverlessd\n"
    printf "  install.sh add <name> install a binding\n"
    exit 1
    ;;
esac
