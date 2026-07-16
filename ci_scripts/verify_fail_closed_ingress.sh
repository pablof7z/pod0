#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
cd "$REPO_ROOT"

production_composition=(
  App/Sources/AppMain.swift
  App/Sources/App/RootView.swift
)

if grep -n 'NostrRelayService' "${production_composition[@]}"; then
  echo "legacy ingress architecture check failed: production composes NostrRelayService" >&2
  exit 1
fi

if ! grep -Fq 'isOn: $settings.nostrEnabled' \
  App/Sources/Features/Settings/Agent/AgentSettingsView.swift; then
  echo "legacy ingress architecture check failed: desired nostrEnabled preference was removed" >&2
  exit 1
fi

echo "Legacy unverified remote-agent ingress is fail-closed; desired preference remains stored"
