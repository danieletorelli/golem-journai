#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${BASE_URL:-http://journai.localhost:9006}"
E2E_HOSTNAME="${E2E_HOSTNAME:-e2e-$(date +%s)}"
E2E_SERVICE="${E2E_SERVICE:-e2e-service}"
WAIT_TIMEOUT="${WAIT_TIMEOUT:-60}"

readonly BASE_URL E2E_HOSTNAME E2E_SERVICE WAIT_TIMEOUT

E2E_BODY=""
E2E_STATUS=""

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

log_step() {
  echo "$1"
}

die() {
  echo "$1" >&2
  exit 1
}

die_with_body() {
  echo "$1" >&2
  if [ -n "${E2E_BODY:-}" ]; then
    printf '%s\n' "$E2E_BODY" >&2
  fi
  exit 1
}

wait_for_http() {
  local url="$1"
  local timeout="$2"
  local start
  start=$(date +%s)
  while true; do
    fetch GET "$url"
    if [ "$E2E_STATUS" != "000" ]; then
      return 0
    fi
    if [ $(( $(date +%s) - start )) -ge "$timeout" ]; then
      die_with_body "Timed out waiting for $url"
    fi
    sleep 2
  done
}

fetch() {
  local method="$1"
  local url="$2"
  local data="${3-}"
  local response
  local curl_status=0
  if [ -n "$data" ]; then
    response=$(curl -sS -H "Content-Type: application/json" -X "$method" "$url" -d "$data" -w $'\n%{http_code}') || curl_status=$?
  else
    response=$(curl -sS -X "$method" "$url" -w $'\n%{http_code}') || curl_status=$?
  fi
  if [ "$curl_status" -ne 0 ]; then
    E2E_STATUS="000"
    E2E_BODY=""
    return 0
  fi
  E2E_STATUS="${response##*$'\n'}"
  E2E_BODY="${response%$'\n'*}"
}

json_escape() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  value="${value//$'\n'/\\n}"
  printf '%s' "$value"
}

json_entry() {
  local ts="$1"
  local priority="$2"
  local message="$3"
  local hostname="${4:-$E2E_HOSTNAME}"
  local boot_id="boot-$E2E_HOSTNAME"
  local machine_id="machine-$E2E_HOSTNAME"
  local escaped_message
  local escaped_hostname
  local escaped_service
  escaped_message=$(json_escape "$message")
  escaped_hostname=$(json_escape "$hostname")
  escaped_service=$(json_escape "$E2E_SERVICE")
  printf '{"boot_id":"%s","machine_id":"%s","hostname":"%s","priority":"%s","message":"%s","date":%s,"runtime_scope":"system","syslog_identifier":"%s","comm":"%s","pid":"1234","uid":"0","gid":"0"}' \
    "$boot_id" \
    "$machine_id" \
    "$escaped_hostname" \
    "$priority" \
    "$escaped_message" \
    "$ts" \
    "$escaped_service" \
    "$escaped_service"
}

build_payload() {
  local base_ts="$1"
  local entries=()
  local i
  for i in 0 1 2 3 4 5; do
    entries+=("$(json_entry "$((base_ts + i))" "3" "error spike $i for $E2E_SERVICE")")
  done
  entries+=("$(json_entry "$((base_ts + 6))" "6" "info ok for $E2E_SERVICE")")
  entries+=("$(json_entry "$((base_ts + 7))" "9" "invalid priority entry")")
  local joined
  joined=$(IFS=,; printf '%s' "${entries[*]}")
  printf '{"entries":[%s]}' "$joined"
}

assert_status() {
  local status="$1"
  local method="$2"
  local url="$3"
  if [ "$status" != "200" ]; then
    die_with_body "$method $url failed with status $status"
  fi
}

assert_json_success() {
  local context="$1"
  if ! jq -e '.success == true' <<<"$E2E_BODY" >/dev/null; then
    die_with_body "$context"
  fi
}

expect_body_contains() {
  local needle="$1"
  if ! grep -Fq "$needle" <<<"$E2E_BODY"; then
    die_with_body "Expected response to contain: $needle"
  fi
}

assert_collected_count() {
  local expected="$1"
  local message
  message=$(jq -r '.message // ""' <<<"$E2E_BODY")
  if [[ "$message" =~ Collected[[:space:]]+([0-9]+)[[:space:]]+entries ]]; then
    if [ "${BASH_REMATCH[1]}" -ne "$expected" ]; then
      die_with_body "Unexpected collect count: $message"
    fi
  else
    die_with_body "Unexpected collect message: $message"
  fi
}

expect_entries_count() {
  local url="$1"
  local expected="$2"
  local count
  fetch GET "$url"
  assert_status "$E2E_STATUS" "GET" "$url"
  assert_json_success "Entries response marked as failure"
  count=$(jq -r '.results.count // empty' <<<"$E2E_BODY")
  if ! [[ "$count" =~ ^[0-9]+$ ]]; then
    die_with_body "Invalid entries count: $count"
  fi
  if [ "$count" -ne "$expected" ]; then
    die_with_body "Expected $expected entries, got $count"
  fi
}

expect_error_spikes() {
  local url="$1"
  local service="$2"
  local expected="$3"
  local service_count
  fetch GET "$url"
  assert_status "$E2E_STATUS" "GET" "$url"
  assert_json_success "Error spikes response marked as failure"
  if ! jq -e '.results | length > 0' <<<"$E2E_BODY" >/dev/null; then
    die_with_body "No error spikes returned"
  fi
  service_count=$(jq -r --arg service "$service" '
    .results[]
    | select((.service_name // .["service-name"] // .service) == $service)
    | (.error_count // .["error-count"] // .errorCount)
  ' <<<"$E2E_BODY" | head -n1)
  if [ -z "$service_count" ]; then
    die_with_body "Service $service not found in spikes"
  fi
  if [ "$service_count" -ne "$expected" ]; then
    die_with_body "Unexpected error spike count: $service_count"
  fi
}

wait_for_analysis_id() {
  local url="$1"
  local timeout="$2"
  local start
  local analysis_id
  start=$(date +%s)
  while true; do
    fetch GET "$url"
    if [ "$E2E_STATUS" = "200" ] && ! grep -Fq "No analysis history found" <<<"$E2E_BODY"; then
      analysis_id=$(grep -oE '/analysis/details/[0-9]+' <<<"$E2E_BODY" | head -n1 | awk -F/ '{print $4}')
      if [ -n "$analysis_id" ]; then
        printf '%s' "$analysis_id"
        return 0
      fi
    fi
    if [ $(( $(date +%s) - start )) -ge "$timeout" ]; then
      die_with_body "Timed out waiting for analysis history"
    fi
    sleep 2
  done
}

require_cmd curl
require_cmd jq

log_step "Waiting for $BASE_URL to respond"
wait_for_http "$BASE_URL/dashboard/overview" "$WAIT_TIMEOUT"

base_ts=$(date +%s)
payload=$(build_payload "$base_ts")

log_step "Posting entries to $BASE_URL/collect/$E2E_HOSTNAME"
fetch POST "$BASE_URL/collect/$E2E_HOSTNAME" "$payload"
assert_status "$E2E_STATUS" "POST" "$BASE_URL/collect/$E2E_HOSTNAME"
assert_json_success "Collect response marked as failure"
assert_collected_count 7

expect_entries_count "$BASE_URL/entries/$E2E_HOSTNAME?since=-1&priority=-1&contains=" 7

since_ts=$((base_ts + 4))
expect_entries_count "$BASE_URL/entries/$E2E_HOSTNAME?since=$since_ts&priority=-1&contains=" 3
expect_entries_count "$BASE_URL/entries/$E2E_HOSTNAME?since=-1&priority=3&contains=" 6
expect_entries_count "$BASE_URL/entries/$E2E_HOSTNAME?since=-1&priority=-1&contains=error%20spike" 6
expect_entries_count "$BASE_URL/entries/$E2E_HOSTNAME?since=$since_ts&priority=3&contains=error%20spike" 2

log_step "Checking error spikes"
expect_error_spikes "$BASE_URL/errors/$E2E_HOSTNAME" "$E2E_SERVICE" 6

log_step "Waiting for analysis results"
analysis_id=$(wait_for_analysis_id "$BASE_URL/analysis/history/$E2E_HOSTNAME" "$WAIT_TIMEOUT")

log_step "Checking analysis details"
fetch GET "$BASE_URL/analysis/details/$analysis_id"
assert_status "$E2E_STATUS" "GET" "$BASE_URL/analysis/details/$analysis_id"
expect_body_contains "$E2E_HOSTNAME"
expect_body_contains "$E2E_SERVICE"
expect_body_contains "Needs User Action"
expect_body_contains "Needs User Action:</strong> Yes"

log_step "Checking dashboard overview"
fetch GET "$BASE_URL/dashboard/overview"
assert_status "$E2E_STATUS" "GET" "$BASE_URL/dashboard/overview"
expect_body_contains "/analysis/history/$E2E_HOSTNAME"

log_step "Checking dashboard alerts"
fetch GET "$BASE_URL/dashboard/alerts"
assert_status "$E2E_STATUS" "GET" "$BASE_URL/dashboard/alerts"
expect_body_contains "$E2E_HOSTNAME"
expect_body_contains "$E2E_SERVICE"

log_step "Checking analysis queue"
fetch GET "$BASE_URL/analysis/queue"
assert_status "$E2E_STATUS" "GET" "$BASE_URL/analysis/queue"
expect_body_contains "$E2E_SERVICE"

echo "E2E tests passed."
