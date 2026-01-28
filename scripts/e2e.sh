#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${BASE_URL:-http://journai.localhost:9006}"
E2E_HOSTNAME="${E2E_HOSTNAME:-e2e-$(date +%s)}"
E2E_SERVICE="${E2E_SERVICE:-e2e-service}"
WAIT_TIMEOUT="${WAIT_TIMEOUT:-60}"

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

wait_for_http() {
  local url="$1"
  local timeout="$2"
  local start
  start=$(date +%s)
  while true; do
    local code
    code=$(curl -sS -o /dev/null -w "%{http_code}" "$url" || true)
    if [ "$code" != "000" ]; then
      return 0
    fi
    if [ $(( $(date +%s) - start )) -ge "$timeout" ]; then
      echo "Timed out waiting for $url" >&2
      exit 1
    fi
    sleep 2
  done
}

fetch() {
  local method="$1"
  local url="$2"
  local data="${3-}"
  if [ -n "$data" ]; then
    curl -sS -o "$E2E_BODY" -w "%{http_code}" -H "Content-Type: application/json" -X "$method" "$url" -d "$data"
  else
    curl -sS -o "$E2E_BODY" -w "%{http_code}" -X "$method" "$url"
  fi
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
  printf '{"boot-id":"%s","machine-id":"%s","hostname":"%s","priority":"%s","message":"%s","date":%s,"runtime-scope":"system","syslog-identifier":"%s","comm":"%s","pid":"1234","uid":"0","gid":"0"}' \
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
  printf '[%s]' "$joined"
}

require_cmd curl
require_cmd jq

E2E_BODY="$(mktemp)"
trap 'rm -f "$E2E_BODY"' EXIT

wait_for_http "$BASE_URL/dashboard/overview" "$WAIT_TIMEOUT"

base_ts=$(date +%s)
payload=$(build_payload "$base_ts")

echo "Posting entries to $BASE_URL/collect/$E2E_HOSTNAME"
status=$(fetch POST "$BASE_URL/collect/$E2E_HOSTNAME" "$payload")
if [ "$status" != "200" ]; then
  echo "Collect request failed with status $status" >&2
  cat "$E2E_BODY" >&2
  exit 1
fi

if ! jq -e '.success == true' "$E2E_BODY" >/dev/null; then
  echo "Collect response marked as failure" >&2
  cat "$E2E_BODY" >&2
  exit 1
fi

message=$(jq -r '.message // ""' "$E2E_BODY")
if [[ "$message" =~ Collected[[:space:]]+([0-9]+)[[:space:]]+entries ]]; then
  if [ "${BASH_REMATCH[1]}" -ne 7 ]; then
    echo "Unexpected collect count: $message" >&2
    exit 1
  fi
else
  echo "Unexpected collect message: $message" >&2
  exit 1
fi

expect_entries_count() {
  local url="$1"
  local expected="$2"
  status=$(fetch GET "$url")
  if [ "$status" != "200" ]; then
    echo "GET $url failed with status $status" >&2
    cat "$E2E_BODY" >&2
    exit 1
  fi
  if ! jq -e '.success == true' "$E2E_BODY" >/dev/null; then
    echo "Entries response marked as failure" >&2
    cat "$E2E_BODY" >&2
    exit 1
  fi
  count=$(jq -r '.results.count // empty' "$E2E_BODY")
  if ! [[ "$count" =~ ^[0-9]+$ ]]; then
    echo "Invalid entries count: $count" >&2
    cat "$E2E_BODY" >&2
    exit 1
  fi
  if [ "$count" -ne "$expected" ]; then
    echo "Expected $expected entries, got $count" >&2
    exit 1
  fi
}

expect_entries_count "$BASE_URL/entries/$E2E_HOSTNAME?since=-1&priority=-1&contains=" 7

since_ts=$((base_ts + 4))
expect_entries_count "$BASE_URL/entries/$E2E_HOSTNAME?since=$since_ts&priority=-1&contains=" 3

expect_entries_count "$BASE_URL/entries/$E2E_HOSTNAME?since=-1&priority=3&contains=" 6

expect_entries_count "$BASE_URL/entries/$E2E_HOSTNAME?since=-1&priority=-1&contains=error%20spike" 6

expect_entries_count "$BASE_URL/entries/$E2E_HOSTNAME?since=$since_ts&priority=3&contains=error%20spike" 2

echo "Checking error spikes"
status=$(fetch GET "$BASE_URL/errors/$E2E_HOSTNAME")
if [ "$status" != "200" ]; then
  echo "GET /errors failed with status $status" >&2
  cat "$E2E_BODY" >&2
  exit 1
fi

if ! jq -e '.success == true' "$E2E_BODY" >/dev/null; then
  echo "Error spikes response marked as failure" >&2
  cat "$E2E_BODY" >&2
  exit 1
fi
if ! jq -e '.results | length > 0' "$E2E_BODY" >/dev/null; then
  echo "No error spikes returned" >&2
  cat "$E2E_BODY" >&2
  exit 1
fi
service_count=$(jq -r --arg service "$E2E_SERVICE" '
  .results[]
  | select((.service_name // .["service-name"] // .service) == $service)
  | (.error_count // .["error-count"] // .errorCount)
' "$E2E_BODY" | head -n1)
if [ -z "$service_count" ]; then
  echo "Service $E2E_SERVICE not found in spikes" >&2
  cat "$E2E_BODY" >&2
  exit 1
fi
if [ "$service_count" -ne 6 ]; then
  echo "Unexpected error spike count: $service_count" >&2
  exit 1
fi

echo "Waiting for analysis results"
analysis_id=""
start=$(date +%s)
while true; do
  status=$(fetch GET "$BASE_URL/analysis/history/$E2E_HOSTNAME")
  if [ "$status" = "200" ] && ! grep -q "No analysis history found" "$E2E_BODY"; then
    analysis_id=$(grep -oE '/analysis/details/[0-9]+' "$E2E_BODY" | head -n1 | awk -F/ '{print $4}')
    if [ -n "$analysis_id" ]; then
      break
    fi
  fi
  if [ $(( $(date +%s) - start )) -ge "$WAIT_TIMEOUT" ]; then
    echo "Timed out waiting for analysis history" >&2
    cat "$E2E_BODY" >&2
    exit 1
  fi
  sleep 2
done

echo "Checking analysis details"
status=$(fetch GET "$BASE_URL/analysis/details/$analysis_id")
if [ "$status" != "200" ]; then
  echo "GET /analysis/details failed with status $status" >&2
  cat "$E2E_BODY" >&2
  exit 1
fi
grep -q "$E2E_HOSTNAME" "$E2E_BODY"
grep -q "$E2E_SERVICE" "$E2E_BODY"
grep -q "Needs User Action" "$E2E_BODY"
grep -q "Needs User Action:</strong> Yes" "$E2E_BODY"

echo "Checking dashboard overview"
status=$(fetch GET "$BASE_URL/dashboard/overview")
if [ "$status" != "200" ]; then
  echo "GET /dashboard/overview failed with status $status" >&2
  cat "$E2E_BODY" >&2
  exit 1
fi
grep -q "/analysis/history/$E2E_HOSTNAME" "$E2E_BODY"

echo "Checking dashboard alerts"
status=$(fetch GET "$BASE_URL/dashboard/alerts")
if [ "$status" != "200" ]; then
  echo "GET /dashboard/alerts failed with status $status" >&2
  cat "$E2E_BODY" >&2
  exit 1
fi
grep -q "$E2E_HOSTNAME" "$E2E_BODY"
grep -q "$E2E_SERVICE" "$E2E_BODY"

echo "Checking analysis queue"
status=$(fetch GET "$BASE_URL/analysis/queue")
if [ "$status" != "200" ]; then
  echo "GET /analysis/queue failed with status $status" >&2
  cat "$E2E_BODY" >&2
  exit 1
fi
grep -q "$E2E_SERVICE" "$E2E_BODY"

echo "E2E tests passed."
