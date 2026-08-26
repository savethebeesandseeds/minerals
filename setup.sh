#!/usr/bin/env bash
# Dockerfile-free bootstrap for the Minerals admin and selector-review services.
#
# Compose mounts this script read-only into a pinned Rust base image. Package
# installation and builds happen only inside that container; host execution is
# rejected. Each service drops permanently to its own non-root identity with an
# empty capability bounding set before opening a network listener.

set -Eeuo pipefail
IFS=$'\n\t'
umask 022

readonly EXPECTED_CONTEXT='waajacu-minerals-runtime-v1'
readonly EXPECTED_BASE='rust:1.96-bookworm@sha256:a339861ae23e9abb272cea45dfafde21760d2ce6577a70f8a926153677902663'
readonly BOOTSTRAP_SCRIPT='/bootstrap/setup.sh'
readonly BOOTSTRAP_COMPOSE='/bootstrap/compose.yaml'
readonly NGINX_TEMPLATE='/bootstrap/minerals-selector-review.conf'
readonly SOURCE_ROOT='/workspace'
readonly RUNTIME_ROOT='/runtime'
readonly DATA_ROOT_EXPECTED='/app/data'
readonly BUILD_HOME='/tmp/minerals-build-home'
readonly TARGET_ROOT="${CARGO_TARGET_DIR:-/build/target}"

die() {
  printf 'setup.sh: %s\n' "$*" >&2
  exit 1
}

note() {
  printf '\n==> %s\n' "$*"
}

require_decimal_identity() {
  local name=$1
  local value=$2
  [[ "$value" =~ ^[0-9]+$ ]] || die "$name must be a decimal integer"
  (( value >= 1 && value <= 2147483647 )) ||
    die "$name is outside the non-root range"
}

require_container_context() {
  [[ -f /.dockerenv ]] || die 'refusing to run outside a Docker container'
  [[ "${MINERALS_CONTAINER_CONTEXT:-}" == "$EXPECTED_CONTEXT" ]] ||
    die 'missing or incorrect MINERALS_CONTAINER_CONTEXT marker'
  [[ "${MINERALS_BASE_IMAGE:-}" == "$EXPECTED_BASE" ]] ||
    die 'the declared base image differs from the audited image digest'
  [[ -r "$BOOTSTRAP_SCRIPT" && -r "$BOOTSTRAP_COMPOSE" ]] ||
    die 'the read-only bootstrap mounts are missing'
  [[ ! -e /var/run/docker.sock ]] ||
    die 'the Docker socket must never be mounted into a Minerals service'
}

parse_mode() {
  case "${1:-}" in
    admin|web) MINERALS_MODE=$1 ;;
    *) die 'mode must be exactly admin or web' ;;
  esac

  MINERALS_BUILD_UID="${MINERALS_BUILD_UID:-10001}"
  MINERALS_BUILD_GID="${MINERALS_BUILD_GID:-10001}"
  MINERALS_RUNTIME_UID="${MINERALS_RUNTIME_UID:-10001}"
  MINERALS_RUNTIME_GID="${MINERALS_RUNTIME_GID:-10001}"
  require_decimal_identity MINERALS_BUILD_UID "$MINERALS_BUILD_UID"
  require_decimal_identity MINERALS_BUILD_GID "$MINERALS_BUILD_GID"
  require_decimal_identity MINERALS_RUNTIME_UID "$MINERALS_RUNTIME_UID"
  require_decimal_identity MINERALS_RUNTIME_GID "$MINERALS_RUNTIME_GID"
  export MINERALS_MODE MINERALS_BUILD_UID MINERALS_BUILD_GID
  export MINERALS_RUNTIME_UID MINERALS_RUNTIME_GID
}

install_dependencies() {
  local -a packages=(ca-certificates curl util-linux)
  if [[ "$MINERALS_MODE" == web ]]; then
    packages+=(nginx)
  fi

  if command -v curl >/dev/null 2>&1 &&
     command -v setpriv >/dev/null 2>&1 &&
     { [[ "$MINERALS_MODE" != web ]] || command -v nginx >/dev/null 2>&1; }; then
    return
  fi

  local -a apt_options=(
    -o APT::Get::AllowUnauthenticated=false
    -o Acquire::AllowInsecureRepositories=false
    -o Acquire::AllowDowngradeToInsecureRepositories=false
    -o Acquire::Check-Valid-Until=true
    -o APT::Install-Recommends=false
    -o APT::Install-Suggests=false
  )
  local -a clean_environment=(
    'PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin'
    'HOME=/root'
    'LANG=C.UTF-8'
    'LC_ALL=C.UTF-8'
    'DEBIAN_FRONTEND=noninteractive'
  )
  local -a proxy_environment=()
  local name
  for name in HTTP_PROXY HTTPS_PROXY NO_PROXY http_proxy https_proxy no_proxy; do
    if [[ -n "${!name:-}" ]]; then
      proxy_environment+=("$name=${!name}")
    fi
  done

  note "Installing the explicit $MINERALS_MODE dependency profile inside the container"
  env -i "${clean_environment[@]}" "${proxy_environment[@]}" \
    apt-get "${apt_options[@]}" update
  env -i "${clean_environment[@]}" "${proxy_environment[@]}" \
    apt-get "${apt_options[@]}" install -y --no-install-recommends "${packages[@]}"
  env -i "${clean_environment[@]}" "${proxy_environment[@]}" \
    apt-get "${apt_options[@]}" check
  [[ -z $(env -i "${clean_environment[@]}" dpkg --audit) ]] ||
    die 'dpkg reports an incomplete package state'
}

assert_read_only_mount() {
  local path=$1
  local options
  options=$(findmnt -T "$path" -n -o OPTIONS) ||
    die "cannot inspect the mount containing $path"
  [[ ",$options," == *,ro,* ]] || die "$path is not mounted read-only"
}

validate_source_mounts() {
  local -a inputs=(
    "$BOOTSTRAP_SCRIPT"
    "$BOOTSTRAP_COMPOSE"
    "$SOURCE_ROOT/Cargo.toml"
    "$SOURCE_ROOT/Cargo.lock"
    "$SOURCE_ROOT/askama.toml"
    "$SOURCE_ROOT/crates"
    "$SOURCE_ROOT/src"
    "$SOURCE_ROOT/static"
  )
  if [[ "$MINERALS_MODE" == web ]]; then
    inputs+=(
      "$SOURCE_ROOT/public-app"
      "$SOURCE_ROOT/public-catalog"
      "$NGINX_TEMPLATE"
    )
  fi

  local input
  for input in "${inputs[@]}"; do
    [[ -e "$input" ]] || die "required source mount is absent: $input"
    assert_read_only_mount "$input"
  done
  [[ ! -e "$SOURCE_ROOT/.git" ]] ||
    die 'Git metadata must not enter the runtime container'
  [[ ! -e "$SOURCE_ROOT/.env" && ! -e "$SOURCE_ROOT/.env.local" ]] ||
    die 'environment files must be injected, never source-mounted'
}

prepare_build_cache() {
  install -d -m 0755 \
    /usr/local/cargo/registry \
    /usr/local/cargo/git \
    "${CARGO_TARGET_DIR:-/build/target}" \
    "$BUILD_HOME"
  chown "$MINERALS_BUILD_UID:$MINERALS_BUILD_GID" /usr/local/cargo
  chown -R "$MINERALS_BUILD_UID:$MINERALS_BUILD_GID" \
    /usr/local/cargo/registry \
    /usr/local/cargo/git \
    "${CARGO_TARGET_DIR:-/build/target}" \
    "$BUILD_HOME"
  chmod 0755 /usr/local/cargo
  chmod -R u+rwX,go+rX,go-w \
    /usr/local/cargo/registry \
    /usr/local/cargo/git \
    "$TARGET_ROOT"
}

acquire_build_lock() {
  install -d -m 0755 "$TARGET_ROOT"
  exec 9>>"$TARGET_ROOT/.minerals-build.lock"
  flock 9
}

seal_build_cache() {
  chown root:root /usr/local/cargo
  chown -R root:root \
    /usr/local/cargo/registry \
    /usr/local/cargo/git \
    "$TARGET_ROOT"
  chmod 0555 /usr/local/cargo
  chmod -R a-w \
    /usr/local/cargo/registry \
    /usr/local/cargo/git \
    "$TARGET_ROOT"
  flock -u 9
  exec 9>&-
}

run_as_builder() {
  local -a proxy_environment=()
  local name
  for name in HTTP_PROXY HTTPS_PROXY NO_PROXY http_proxy https_proxy no_proxy; do
    if [[ -n "${!name:-}" ]]; then
      proxy_environment+=("$name=${!name}")
    fi
  done

  setpriv \
    --reuid="$MINERALS_BUILD_UID" \
    --regid="$MINERALS_BUILD_GID" \
    --clear-groups \
    --no-new-privs \
    --bounding-set=-all \
    --inh-caps=-all \
    --ambient-caps=-all \
    env -i \
      'PATH=/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin' \
      "HOME=$BUILD_HOME" \
      'LANG=C.UTF-8' \
      'LC_ALL=C.UTF-8' \
      'CARGO_HOME=/usr/local/cargo' \
      "CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-/build/target}" \
      'RUSTUP_HOME=/usr/local/rustup' \
      'CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse' \
      "${proxy_environment[@]}" \
      "$@"
}

build_admin() {
  note 'Building the locked private admin service'
  (
    cd "$SOURCE_ROOT"
    run_as_builder cargo build --locked --release -p minerals-admin --bin minerals
  )

  install -d -m 0755 "$RUNTIME_ROOT"
  install -m 0555 \
    "${CARGO_TARGET_DIR:-/build/target}/release/minerals" \
    "$RUNTIME_ROOT/minerals.next"
  mv -f "$RUNTIME_ROOT/minerals.next" "$RUNTIME_ROOT/minerals"
  chown root:root "$RUNTIME_ROOT" "$RUNTIME_ROOT/minerals"
  chmod 0555 "$RUNTIME_ROOT" "$RUNTIME_ROOT/minerals"
}

prepare_admin_password() {
  local secret_file="$RUNTIME_ROOT/admin-password"
  local configured="${ADMIN_PASSWORD:-}"
  if (( ${#configured} >= 12 )); then
    if [[ -e "$secret_file" || -L "$secret_file" ]]; then
      [[ -f "$secret_file" && ! -L "$secret_file" ]] ||
        die "$secret_file must be one regular, non-symlink file"
      unlink "$secret_file"
    fi
    return
  fi

  local generated
  if [[ -e "$secret_file" || -L "$secret_file" ]]; then
    [[ -f "$secret_file" && ! -L "$secret_file" ]] ||
      die "$secret_file must be one regular, non-symlink file"
    generated=$(<"$secret_file")
  else
    generated=$(od -An -N24 -tx1 /dev/urandom | tr -d ' \n')
    [[ "$generated" =~ ^[0-9a-f]{48}$ ]] ||
      die 'failed to generate the local admin password'
    printf '%s\n' "$generated" > "$secret_file.next"
    chown root:root "$secret_file.next"
    chmod 0400 "$secret_file.next"
    mv -f "$secret_file.next" "$secret_file"
  fi
  (( ${#generated} >= 24 )) ||
    die 'the persisted generated admin password is invalid'
  export ADMIN_PASSWORD="$generated"
  note 'Using the generated admin password stored in the private admin runtime volume'
}

prepare_private_data() {
  [[ "${DATA_ROOT:-}" == "$DATA_ROOT_EXPECTED" ]] ||
    die "DATA_ROOT must be exactly $DATA_ROOT_EXPECTED in the container"
  [[ -d "$DATA_ROOT_EXPECTED" && ! -L "$DATA_ROOT_EXPECTED" ]] ||
    die "$DATA_ROOT_EXPECTED must be a real directory"

  local mount_uid mount_gid
  mount_uid=$(stat -c '%u' "$DATA_ROOT_EXPECTED")
  mount_gid=$(stat -c '%g' "$DATA_ROOT_EXPECTED")
  [[ "$mount_uid" =~ ^[0-9]+$ && "$mount_gid" =~ ^[0-9]+$ ]] ||
    die 'the private data mount has invalid numeric ownership'

  # Native Linux keeps the real bind-mount owner. Docker Desktop commonly
  # presents a Windows bind as root:root, where the configured fallback is the
  # stable service identity.
  if (( mount_uid != 0 )); then
    MINERALS_RUNTIME_UID=$mount_uid
  fi
  if (( mount_gid != 0 )); then
    MINERALS_RUNTIME_GID=$mount_gid
  fi
  require_decimal_identity MINERALS_RUNTIME_UID "$MINERALS_RUNTIME_UID"
  require_decimal_identity MINERALS_RUNTIME_GID "$MINERALS_RUNTIME_GID"
  export MINERALS_RUNTIME_UID MINERALS_RUNTIME_GID

  note 'Normalizing the private data mount without following symlinks'
  chown --recursive --no-dereference \
    "$MINERALS_RUNTIME_UID:$MINERALS_RUNTIME_GID" "$DATA_ROOT_EXPECTED"
  find "$DATA_ROOT_EXPECTED" -xdev -type d -exec chmod 0700 {} +
  find "$DATA_ROOT_EXPECTED" -xdev -type f -exec chmod 0600 {} +
}

safe_remove_old_web_releases() {
  local active_name=$1
  local candidate
  while IFS= read -r -d '' candidate; do
    [[ "$candidate" == "$RUNTIME_ROOT"/release-* ]] ||
      die "refusing to remove unexpected runtime path: $candidate"
    [[ "${candidate##*/}" != "$active_name" ]] || continue
    rm -rf -- "$candidate"
  done < <(
    find "$RUNTIME_ROOT" -mindepth 1 -maxdepth 1 \
      -type d -name 'release-*' -print0
  )
}

build_web() {
  note 'Building the locked public catalog assembler'
  (
    cd "$SOURCE_ROOT"
    run_as_builder cargo build \
      --locked --release -p minerals-public-catalog --bin export-public
  )

  install -d -m 0755 "$RUNTIME_ROOT"
  chown "$MINERALS_BUILD_UID:$MINERALS_BUILD_GID" "$RUNTIME_ROOT"
  chmod 0755 "$RUNTIME_ROOT"

  local review_session release_name release_path current_tmp
  review_session=$(od -An -N12 -tx1 /dev/urandom | tr -d ' \n')
  [[ "$review_session" =~ ^[0-9a-f]{24}$ ]] ||
    die 'failed to create a safe selector review session'
  release_name="release-$review_session"
  release_path="$RUNTIME_ROOT/$release_name"
  current_tmp="$RUNTIME_ROOT/.current-$review_session"
  [[ ! -e "$release_path" && ! -e "$current_tmp" ]] ||
    die 'fresh web runtime paths unexpectedly already exist'
  if [[ -e "$RUNTIME_ROOT/current" && ! -L "$RUNTIME_ROOT/current" ]]; then
    die "$RUNTIME_ROOT/current must be absent or a symlink"
  fi

  note 'Assembling and validating the committed 6,226-record public catalog'
  (
    cd "$SOURCE_ROOT"
    run_as_builder "${CARGO_TARGET_DIR:-/build/target}/release/export-public" \
      --assemble-catalog "$SOURCE_ROOT/public-catalog" \
      --output "$release_path" \
      --app-root "$SOURCE_ROOT/public-app"
    run_as_builder "${CARGO_TARGET_DIR:-/build/target}/release/export-public" \
      --validate-release "$release_path" \
      --app-root "$SOURCE_ROOT/public-app"
  )

  # This entry is deliberately added only after strict release validation.
  # It therefore remains absent from production exports and their allowlist.
  install -m 0444 \
    "$SOURCE_ROOT/public-app/selector-review.html" \
    "$release_path/selector-review.html"

  sed "s/@REVIEW_SESSION@/$review_session/g" \
    "$NGINX_TEMPLATE" > "$RUNTIME_ROOT/nginx.conf.next"
  if grep -Fq '@REVIEW_SESSION@' "$RUNTIME_ROOT/nginx.conf.next"; then
    die 'the rendered nginx configuration contains an unresolved review-session placeholder'
  fi
  grep -Fq "$review_session" "$RUNTIME_ROOT/nginx.conf.next" ||
    die 'the rendered nginx configuration does not contain the fresh review session'

  chown -R root:root "$release_path"
  find "$release_path" -type d -exec chmod 0555 {} +
  find "$release_path" -type f -exec chmod 0444 {} +
  chown root:root "$RUNTIME_ROOT/nginx.conf.next"
  chmod 0444 "$RUNTIME_ROOT/nginx.conf.next"

  install -d -m 0700 \
    -o "$MINERALS_RUNTIME_UID" -g "$MINERALS_RUNTIME_GID" \
    /tmp/minerals-nginx \
    /tmp/minerals-nginx/client \
    /tmp/minerals-nginx/proxy \
    /tmp/minerals-nginx/fastcgi \
    /tmp/minerals-nginx/uwsgi \
    /tmp/minerals-nginx/scgi
  chown root:root "$RUNTIME_ROOT"
  chmod 0555 "$RUNTIME_ROOT"

  if [[ -L "$RUNTIME_ROOT/current" ]]; then
    local current_release
    current_release=$(readlink "$RUNTIME_ROOT/current")
    [[ "$current_release" =~ ^release-[0-9a-f]{24}$ ]] ||
      die 'the active web release symlink has an invalid target'
    [[ -d "$RUNTIME_ROOT/$current_release" &&
       ! -L "$RUNTIME_ROOT/$current_release" ]] ||
      die 'the active web release target is missing or is not a real directory'
  fi

  # Validate the candidate configuration under the final service identity
  # before replacing either live artifact. Nginx is not running during this
  # bootstrap phase, so the two following renames form one startup commit.
  setpriv \
    --reuid="$MINERALS_RUNTIME_UID" \
    --regid="$MINERALS_RUNTIME_GID" \
    --clear-groups \
    --no-new-privs \
    --bounding-set=-all \
    --inh-caps=-all \
    --ambient-caps=-all \
    nginx -t -c "$RUNTIME_ROOT/nginx.conf.next"

  mv -f "$RUNTIME_ROOT/nginx.conf.next" "$RUNTIME_ROOT/nginx.conf"
  ln -s "$release_name" "$current_tmp"
  mv -Tf "$current_tmp" "$RUNTIME_ROOT/current"
  safe_remove_old_web_releases "$release_name"
}

assert_unprivileged_runtime() {
  [[ "$(id -u)" == "$MINERALS_RUNTIME_UID" ]] ||
    die 'runtime UID does not match the configured identity'
  [[ "$(id -g)" == "$MINERALS_RUNTIME_GID" ]] ||
    die 'runtime GID does not match the configured identity'
  [[ "$(awk '/^NoNewPrivs:/ {print $2}' /proc/self/status)" == 1 ]] ||
    die 'NoNewPrivs is not active in the runtime phase'

  local field value
  for field in CapInh CapPrm CapEff CapBnd CapAmb; do
    value=$(awk -v key="$field:" '$1 == key {print $2}' /proc/self/status)
    [[ "$value" == 0000000000000000 ]] || die "$field is not empty"
  done
}

run_runtime() {
  assert_unprivileged_runtime
  [[ ! -e /var/run/docker.sock ]] ||
    die 'the Docker socket appeared during the runtime phase'

  case "$MINERALS_MODE" in
    admin)
      umask 077
      exec "$RUNTIME_ROOT/minerals"
      ;;
    web)
      umask 022
      exec nginx -c "$RUNTIME_ROOT/nginx.conf" -g 'daemon off;'
      ;;
  esac
}

enter_runtime() {
  export MINERALS_PHASE=runtime
  export HOME=/tmp/minerals-runtime-home
  install -d -m 0700 \
    -o "$MINERALS_RUNTIME_UID" -g "$MINERALS_RUNTIME_GID" \
    "$HOME"
  note "Dropping permanently to uid=$MINERALS_RUNTIME_UID gid=$MINERALS_RUNTIME_GID"
  exec setpriv \
    --reuid="$MINERALS_RUNTIME_UID" \
    --regid="$MINERALS_RUNTIME_GID" \
    --clear-groups \
    --no-new-privs \
    --bounding-set=-all \
    --inh-caps=-all \
    --ambient-caps=-all \
    /bin/bash "$BOOTSTRAP_SCRIPT" "$MINERALS_MODE"
}

main() {
  require_container_context
  parse_mode "${1:-}"

  if [[ "${MINERALS_PHASE:-}" == runtime ]]; then
    run_runtime
  fi

  (( EUID == 0 )) || die 'the setup phase must begin as container root'
  install_dependencies
  validate_source_mounts
  if [[ "$MINERALS_MODE" == web ]]; then
    [[ -r "$NGINX_TEMPLATE" ]] || die 'nginx review template is missing'
  fi

  acquire_build_lock
  prepare_build_cache

  case "$MINERALS_MODE" in
    admin)
      build_admin
      ;;
    web)
      build_web
      ;;
  esac
  seal_build_cache

  if [[ "$MINERALS_MODE" == admin ]]; then
    prepare_admin_password
    prepare_private_data
  fi

  enter_runtime
}

main "$@"
