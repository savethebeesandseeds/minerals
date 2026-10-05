#!/usr/bin/env bash
# Project operations inside the existing private admin container.
set -Eeuo pipefail
umask 077

readonly TASK_ROOT='/workspace'
readonly TASK_TARGET='/build/target'
readonly TASK_NODE='/opt/node-v22.23.3-linux-x64/bin'
readonly TASK_PATH="$TASK_NODE:/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
readonly TASK_PILOT='data/pilots/cod-crystallography-v1'
readonly TASK_PREPARED="$TASK_PILOT/preparation-v1"
task_cache_open=false

fail() { printf 'container-task: %s\n' "$*" >&2; exit 1; }

[[ -f /.dockerenv && "${MINERALS_MODE:-${1:-}}" != web ]] || fail 'use the private admin container'
[[ "${MINERALS_CONTAINER_CONTEXT:-}" == waajacu-minerals-runtime-v1 ]] || fail 'incorrect container context'
[[ "${MINERALS_RUNTIME_UID:-}" == 11001 && "${MINERALS_RUNTIME_GID:-}" == 11001 ]] || fail 'unexpected private service identity'
(( EUID == 0 )) || fail 'invoke with docker compose exec -T --user 0:0 admin'
[[ ! -e /var/run/docker.sock && ! -e "$TASK_ROOT/.git" ]] || fail 'unexpected privileged mount'
task_private_uid=$(stat -c '%u' /app/data)
task_private_gid=$(stat -c '%g' /app/data)
[[ "$task_private_uid" =~ ^[0-9]+$ && "$task_private_gid" =~ ^[0-9]+$ ]] || fail 'invalid data mount owner'
(( task_private_uid != 0 )) || task_private_uid=11001
(( task_private_gid != 0 )) || task_private_gid=11001

for task_input in docs schemas tools examples public-app public-catalog; do
  task_options=$(findmnt -T "$TASK_ROOT/$task_input" -n -o OPTIONS)
  [[ ",$task_options," == *,ro,* ]] || fail "source input is not read-only: $task_input"
done
cd "$TASK_ROOT"

run_builder() {
  setpriv --reuid=10001 --regid=10001 --clear-groups --no-new-privs \
    --bounding-set=-all --inh-caps=-all --ambient-caps=-all \
    env -i "PATH=$TASK_PATH" LANG=C.UTF-8 LC_ALL=C.UTF-8 \
    CARGO_HOME=/usr/local/cargo RUSTUP_HOME=/usr/local/rustup \
    CARGO_TARGET_DIR="$TASK_TARGET" CARGO_BUILD_JOBS=2 \
    PYTHONDONTWRITEBYTECODE=1 "$@"
}

run_private() {
  setpriv --reuid="$task_private_uid" --regid="$task_private_gid" --clear-groups --no-new-privs \
    --bounding-set=-all --inh-caps=-all --ambient-caps=-all \
    env -i "PATH=$TASK_PATH" LANG=C.UTF-8 LC_ALL=C.UTF-8 "$@"
}

seal_cache() {
  if [[ "$task_cache_open" == true ]]; then
    chown root:root /usr/local/cargo
    chown -R root:root /usr/local/cargo/registry /usr/local/cargo/git "$TASK_TARGET"
    chmod 0555 /usr/local/cargo
    chmod -R a-w /usr/local/cargo/registry /usr/local/cargo/git "$TASK_TARGET"
    task_cache_open=false
    flock -u 9
    exec 9>&-
  fi
}
trap seal_cache EXIT

open_cache() {
  exec 9>>"$TASK_TARGET/.minerals-build.lock"
  flock 9
  task_cache_open=true
  chown 10001:10001 /usr/local/cargo
  chown -R 10001:10001 /usr/local/cargo/registry /usr/local/cargo/git "$TASK_TARGET"
  chmod 0755 /usr/local/cargo
  chmod -R u+rwX,go+rX,go-w /usr/local/cargo/registry /usr/local/cargo/git "$TASK_TARGET"
}

contracts() {
  run_builder node tools/validate-mineral-content-contracts.mjs .
  run_builder node tools/validate-cod-crystallography-pilot.mjs .
  run_builder node --test tools/test_validate_mineral_content_contracts.mjs \
    tools/test_validate_cod_crystallography_pilot.mjs
}

case "${1:-}" in
  contracts)
    (( $# == 1 )) || fail 'contracts takes no arguments'
    contracts
    ;;
  validate)
    (( $# == 1 )) || fail 'validate takes no arguments'
    contracts
    run_builder python3 -B -m unittest tools/test_check_public_boundary.py
    run_builder node --test tools/test_validate_public_catalog_sidecars.mjs
    open_cache
    run_builder cargo fmt --all -- --check
    run_builder cargo test --locked --workspace
    run_builder cargo clippy --locked --workspace --all-targets -- -D warnings
    run_builder cargo build --locked -p minerals-cod-pilot --bin cod-pilot \
      -p minerals-public-catalog --bin export-public
    task_release_parent=$(mktemp -d /tmp/minerals-validation.XXXXXXXX)
    chown 10001:10001 "$task_release_parent"
    run_builder "$TASK_TARGET/debug/export-public" --assemble-catalog public-catalog \
      --output "$task_release_parent/release" --app-root public-app
    run_builder env WAAJACU_CATALOG_SMOKE_DIR="$task_release_parent/release" \
      node --test public-app/tests.mjs
    seal_cache
    printf 'Container validation passed. Reviewed release: %s\n' "$task_release_parent/release"
    ;;
  pilot-verify|pilot-fetch|pilot-recover-transport)
    task_command=$1
    shift
    task_extra=()
    if [[ "$task_command" == pilot-recover-transport ]]; then
      (( $# == 4 || $# == 6 )) && [[ "$1" == --reviewer && "$3" == --reason ]] || fail 'pilot-recover-transport requires --reviewer TEXT --reason TEXT [--failure-log PRIVATE_RUN_LOG]'
      if (( $# == 6 )); then [[ "$5" == --failure-log ]] || fail 'only --failure-log may follow the review'; fi
      task_extra=("$@")
    elif [[ "$task_command" == pilot-fetch && $# == 2 && "$1" == --max-new-requests ]]; then
      [[ "$2" =~ ^[1-9][0-9]{0,3}$ ]] && (( 10#$2 <= 1000 )) || fail 'request bound must be 1–1000'
      task_extra=(--max-new-requests "$2")
    elif (( $# != 0 )); then
      fail 'pilot-fetch accepts only --max-new-requests N; pilot-verify takes no arguments'
    fi
    open_cache
    run_builder cargo build --locked -p minerals-cod-pilot --bin cod-pilot
    seal_cache
    if [[ "$task_command" == pilot-recover-transport ]]; then
      run_private "$TASK_TARGET/debug/cod-pilot" recover-transport --repo-root . \
        --prepared "$TASK_PREPARED" --pilot-root "$TASK_PILOT" "${task_extra[@]}"
    elif [[ "$task_command" == pilot-verify ]]; then
      run_private "$TASK_TARGET/debug/cod-pilot" verify --repo-root . --input "$TASK_PREPARED"
      run_private "$TASK_TARGET/debug/cod-pilot" verify-execution --repo-root . \
        --prepared "$TASK_PREPARED" --pilot-root "$TASK_PILOT"
    else
      # fetch verifies preparation, prior receipts, and raw bytes before I/O.
      run_private "$TASK_TARGET/debug/cod-pilot" fetch --repo-root . \
        --prepared "$TASK_PREPARED" --pilot-root "$TASK_PILOT" "${task_extra[@]}"
      run_private "$TASK_TARGET/debug/cod-pilot" verify-execution --repo-root . \
        --prepared "$TASK_PREPARED" --pilot-root "$TASK_PILOT"
    fi
    ;;
  *) fail 'expected contracts, validate, pilot-verify, pilot-fetch, or pilot-recover-transport' ;;
esac
