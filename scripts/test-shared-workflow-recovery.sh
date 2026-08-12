#!/bin/zsh
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
evidence_dir="$(mktemp -d "${TMPDIR:-/tmp}/pod0-workflow-recovery.XXXXXX")"
trap 'rm -rf "$evidence_dir"' EXIT
cd "$repo_root/rust"

run_test() {
  local package="$1"
  local test_name="$2"
  local log="$evidence_dir/${package}-${test_name}.log"
  cargo test -p "$package" --lib "$test_name" -- --exact --nocapture 2>&1 | tee "$log"
  grep -F "test $test_name ... ok" "$log" >/dev/null
}

run_test pod0-facade \
  runtime_chapter_workflow_race_tests::process_restart_after_http_success_reissues_until_durable_commit
run_test pod0-facade \
  runtime_download_admission_tests::waiting_request_is_admitted_by_environment_and_survives_restart
run_test pod0-facade \
  transcript_workflow_cutover_tests::legacy_workflow_cutover_survives_each_restart_and_recovers_owned_work
run_test pod0-storage \
  feed_discovery_store_tests::feed_discovery_commit_is_exact_replayable_and_durable_across_restart
run_test pod0-facade \
  runtime_scheduled_agent::workflow_tests::requested_restart_reissues_exactly_once_and_accepted_restart_is_ambiguous
run_test pod0-facade \
  runtime_agent_modules::tests::native_action_is_fenced_and_restart_never_blindly_replays_it
run_test pod0-facade \
  runtime_publication_tests::generated_episode_publication_hands_off_to_nmp_and_persists_receipt_across_restart
run_test pod0-facade \
  runtime_feed_workflow_recovery_tests::interrupted_subscribe_reissues_fetch_after_restart_and_applies_once

echo "shared Rust workflow recovery passed"
