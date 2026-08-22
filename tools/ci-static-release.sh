#!/usr/bin/env bash
set -Eeuo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
temp_parent="${RUNNER_TEMP:-${TMPDIR:-/tmp}}"
work_root="$(mktemp -d "${temp_parent%/}/waajacu-static-release.XXXXXX")"
data_root="$work_root/private-data"
release_parent="$work_root/releases"
release="$release_parent/fixture-v1"
server_log="$work_root/admin.log"
server_pid=""
server_port="${WAAJACU_CI_PORT:-17983}"

case "$server_port" in
  ''|*[!0-9]*)
    echo "WAAJACU_CI_PORT must be a decimal TCP port." >&2
    exit 2
    ;;
esac
if ((server_port < 1024 || server_port > 65535)); then
  echo "WAAJACU_CI_PORT must be between 1024 and 65535." >&2
  exit 2
fi

stop_server() {
  if [[ -n "$server_pid" ]] && kill -0 "$server_pid" 2>/dev/null; then
    kill -TERM "$server_pid"
    wait "$server_pid" || true
  fi
}
trap stop_server EXIT

mkdir -p "$data_root/minerals" "$release_parent"

# The checked-in legacy seed is immutable public test input. Work only on a
# private copy, and give it the search term exercised by public-app/tests.mjs.
cp -a "$repo_root/data/minerals/." "$data_root/minerals/"
while IFS= read -r -d '' json_file; do
  sed -i 's/Phenakite/Quartz/g; s/phenakite/quartz/g' "$json_file"
done < <(find "$data_root/minerals" -type f -name '*.json' -print0)
grep -R -q '"common_name": "Quartz"' "$data_root/minerals"

DATA_ROOT="$data_root" \
ADMIN_PASSWORD="ci-fixture-password-only" \
BIND_ADDRESS="127.0.0.1" \
PORT="$server_port" \
RUST_LOG="minerals=warn" \
  "$repo_root/target/debug/minerals" >"$server_log" 2>&1 &
server_pid=$!

ready=false
for _ in $(seq 1 120); do
  if curl --fail --silent "http://127.0.0.1:$server_port/readyz" >/dev/null; then
    ready=true
    break
  fi
  if ! kill -0 "$server_pid" 2>/dev/null; then
    break
  fi
  sleep 0.25
done

if [[ "$ready" != true ]]; then
  echo "The isolated admin fixture did not become ready." >&2
  cat "$server_log" >&2
  exit 1
fi

kill -TERM "$server_pid"
wait "$server_pid" || true
server_pid=""

"$repo_root/target/debug/export-public" \
  --data-root "$data_root" \
  --output "$release" \
  --app-root "$repo_root/public-app"

test -f "$release/catalog-manifest.json"
test ! -e "$release/minerals.db"

WAAJACU_CATALOG_SMOKE_DIR="$release" \
  node --test "$repo_root/public-app/tests.mjs"

echo "Static release smoke passed: $release"
