#!/usr/bin/env bash
#
# Launch the real podcast-tui in tmux for live GLM/Ollama Cloud validation.
#
# This script intentionally does not fake agent responses or drive kernel calls
# directly. It creates an isolated data dir, starts the real TUI in tmux, and
# writes pane captures that can be attached to validation notes or PRs.
#
# Usage:
#   tests/integration/run_tui_glm_tmux_validation.sh
#   TUI_GLM_SESSION=podcast-tui-arch tests/integration/run_tui_glm_tmux_validation.sh
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

SESSION="${TUI_GLM_SESSION:-podcast-tui-glm-architecture}"
DATA_DIR="${TUI_GLM_DATA_DIR:-/tmp/${SESSION}-data}"
CAPTURE_DIR="${TUI_GLM_CAPTURE_DIR:-/tmp/${SESSION}-captures}"
MODEL="${TUI_GLM_MODEL:-glm-5.1:cloud}"

if ! command -v tmux >/dev/null 2>&1; then
  echo "tmux is required for this validation pass" >&2
  exit 1
fi

if ! command -v ollama >/dev/null 2>&1; then
  echo "ollama is required for live GLM validation" >&2
  exit 1
fi

if ! ollama ls | awk '{print $1}' | grep -Fxq "${MODEL}"; then
  echo "ollama model ${MODEL} is not listed by 'ollama ls'" >&2
  echo "Expected an authenticated local Ollama daemon with cloud models." >&2
  exit 1
fi

rm -rf "${DATA_DIR}" "${CAPTURE_DIR}"
mkdir -p "${DATA_DIR}" "${CAPTURE_DIR}"

echo "==> Building podcast-tui"
cargo build --manifest-path "${REPO_ROOT}/Cargo.toml" -p podcast-tui --bin podcast-tui

if tmux has-session -t "${SESSION}" 2>/dev/null; then
  echo "tmux session ${SESSION} already exists; kill it or set TUI_GLM_SESSION" >&2
  exit 1
fi

echo "==> Starting tmux session ${SESSION}"
tmux new-session -d -s "${SESSION}" -x 140 -y 44 \
  "cd '${REPO_ROOT}' && RUST_BACKTRACE=1 '${REPO_ROOT}/target/debug/podcast-tui' --data-dir '${DATA_DIR}'"

sleep 2
tmux capture-pane -t "${SESSION}" -p -S - >"${CAPTURE_DIR}/00-launch.txt"

cat <<EOF
Live TUI validation session is running.

Session:     ${SESSION}
Data dir:    ${DATA_DIR}
Captures:    ${CAPTURE_DIR}
Model:       ${MODEL}

Attach:
  tmux attach -t ${SESSION}

Capture current pane:
  tmux capture-pane -t ${SESSION} -p -S - > ${CAPTURE_DIR}/NN-description.txt

Send a key or text from another shell:
  tmux send-keys -t ${SESSION} Tab
  tmux send-keys -t ${SESSION} 'message text' Enter

Quit and leave captures/data for inspection:
  tmux send-keys -t ${SESSION} q

EOF
