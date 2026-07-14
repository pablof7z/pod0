#!/usr/bin/env bash
set -euo pipefail

KEYCHAIN_PATH="${KEYCHAIN_PATH:-${RUNNER_TEMP:-/tmp}/app-signing.keychain-db}"

if [[ -f "$KEYCHAIN_PATH" ]]; then
  security delete-keychain "$KEYCHAIN_PATH"
  echo "Deleted temporary keychain."
fi

if [[ -n "${APP_STORE_CONNECT_KEY_ID:-}" ]]; then
  AUTH_KEY_PATH="$HOME/.appstoreconnect/private_keys/AuthKey_${APP_STORE_CONNECT_KEY_ID}.p8"
  if [[ -f "$AUTH_KEY_PATH" ]]; then
    rm -f "$AUTH_KEY_PATH"
    rmdir "$HOME/.appstoreconnect/private_keys" 2>/dev/null || true
    echo "Deleted temporary App Store Connect API key."
  fi
fi

security list-keychains -d user -s \
  "$HOME/Library/Keychains/login.keychain-db" \
  /Library/Keychains/System.keychain

security default-keychain -s "$HOME/Library/Keychains/login.keychain-db"

echo "Restored system keychains."
