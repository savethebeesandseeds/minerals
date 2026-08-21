#!/bin/sh
set -eu

# Everything below /app/data is private application state. This umask also
# applies after setpriv execs the Rust service.
umask 077

if [ "$(id -u)" -ne 0 ]; then
    exec /usr/local/bin/minerals "$@"
fi

data_root="${DATA_ROOT:-/app/data}"
if [ "$data_root" != "/app/data" ]; then
    echo "entrypoint refuses root permission changes outside /app/data" >&2
    exit 1
fi
if [ ! -d "$data_root" ] || [ -L "$data_root" ]; then
    echo "/app/data must be a real directory" >&2
    exit 1
fi

run_uid="$(stat -c '%u' "$data_root")"
run_gid="$(stat -c '%g' "$data_root")"
case "$run_uid" in
    ''|*[!0-9]*) echo "invalid /app/data owner UID" >&2; exit 1 ;;
esac
case "$run_gid" in
    ''|*[!0-9]*) echo "invalid /app/data owner GID" >&2; exit 1 ;;
esac

# Docker Desktop commonly presents a bind mount as root-owned. Use the image's
# fixed service identity in that case; a native Linux bind mount instead keeps
# the owning host user's numeric identity.
if [ "$run_uid" -eq 0 ]; then
    run_uid=10001
fi
if [ "$run_gid" -eq 0 ]; then
    run_gid=10001
fi

# Never follow a data-tree symlink while normalizing ownership. `find` likewise
# changes only actual regular files and directories under this exact mount.
chown --recursive --no-dereference "$run_uid:$run_gid" "$data_root"
find "$data_root" -type d -exec chmod 0700 {} +
find "$data_root" -type f -exec chmod 0600 {} +

# The bootstrap has only the Compose-granted capabilities needed above. Drop
# its identity and the complete capability bounding set before application code
# starts.
exec setpriv \
    --bounding-set=-all \
    --inh-caps=-all \
    --ambient-caps=-all \
    --reuid "$run_uid" \
    --regid "$run_gid" \
    --clear-groups \
    -- /usr/local/bin/minerals "$@"
