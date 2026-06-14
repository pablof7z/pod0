#!/usr/bin/env bash

kill_process_tree() {
  local signal="$1"
  local pid="$2"
  local child

  while IFS= read -r child; do
    kill_process_tree "$signal" "$child"
  done < <(pgrep -P "$pid" 2>/dev/null || true)

  kill "-$signal" "$pid" 2>/dev/null || true
}

run_command_with_timeout() {
  local label="$1"
  local timeout_seconds="$2"
  shift 2

  local timeout_marker
  timeout_marker="$(mktemp)"
  rm -f "$timeout_marker"

  "$@" &
  local command_pid=$!

  (
    sleep "$timeout_seconds"
    if kill -0 "$command_pid" 2>/dev/null; then
      echo "error: ${label} exceeded ${timeout_seconds}s; terminating stalled process tree" >&2
      : > "$timeout_marker"
      kill_process_tree TERM "$command_pid"
      sleep 10
      kill_process_tree KILL "$command_pid"
    fi
  ) &
  local watchdog_pid=$!

  local restore_errexit=0
  case "$-" in
    *e*) restore_errexit=1 ;;
  esac

  set +e
  wait "$command_pid"
  local status=$?
  if [ "$restore_errexit" -eq 1 ]; then
    set -e
  fi

  kill "$watchdog_pid" 2>/dev/null || true
  wait "$watchdog_pid" 2>/dev/null || true

  if [ -e "$timeout_marker" ]; then
    rm -f "$timeout_marker"
    return 124
  fi

  rm -f "$timeout_marker"
  return "$status"
}
