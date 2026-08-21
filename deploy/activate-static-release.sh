#!/bin/sh

# Activate one already-exported public catalog release.
#
# Layout:
#   DEPLOY_ROOT/
#   |-- releases/RELEASE_ID/
#   `-- current -> releases/RELEASE_ID
#
# The final mv is a same-directory rename of a symlink and is atomic on the
# supported Linux filesystems. GNU coreutils and sha256sum are required.

set -eu

fail() {
    printf 'activate-static-release: %s\n' "$*" >&2
    exit 1
}

usage() {
    printf 'Usage: %s DEPLOY_ROOT RELEASE_ID\n' "$0" >&2
    exit 2
}

[ "$#" -eq 2 ] || usage

deploy_root_input=$1
release_id=$2

case "$release_id" in
    ''|.*|*[!A-Za-z0-9._-]*)
        fail "RELEASE_ID must contain only letters, digits, dot, underscore, or hyphen and must not start with a dot"
        ;;
esac

command -v sha256sum >/dev/null 2>&1 || fail "sha256sum is required"
command -v mv >/dev/null 2>&1 || fail "mv is required"

[ -d "$deploy_root_input" ] || fail "deployment root does not exist: $deploy_root_input"
deploy_root=$(CDPATH='' cd -- "$deploy_root_input" && pwd -P) || fail "cannot resolve deployment root"
releases_dir=$deploy_root/releases
release_dir=$releases_dir/$release_id

[ -d "$releases_dir" ] || fail "releases directory does not exist: $releases_dir"
[ -d "$release_dir" ] || fail "release does not exist: $release_dir"
[ ! -L "$release_dir" ] || fail "release directory must not be a symlink: $release_dir"

for relative_path in \
    index.html \
    app.css \
    app.js \
    catalog-worker.js \
    catalog-manifest.json \
    vendor/sqlite/index.mjs \
    vendor/sqlite/sqlite3.wasm
do
    [ -s "$release_dir/$relative_path" ] || fail "required release file is absent or empty: $relative_path"
done

first_symlink=$(find "$release_dir" -type l -print -quit)
[ -z "$first_symlink" ] || fail "release contains a symlink: $first_symlink"

first_private_file=$(find "$release_dir" -type f \( \
    -name 'minerals.db' -o \
    -name '*.db-wal' -o \
    -name '*.db-shm' -o \
    -name '*.db-journal' -o \
    -name '*.sqlite-wal' -o \
    -name '*.sqlite-shm' -o \
    -name '*.sqlite-journal' -o \
    -name '*.sqlite3-wal' -o \
    -name '*.sqlite3-shm' -o \
    -name '*.sqlite3-journal' \
\) -print -quit)
[ -z "$first_private_file" ] || fail "release contains a private or mutable database artifact: $first_private_file"

# The exporter guarantees exactly one public database. Recheck the filename,
# byte count declared by the manifest, and its content-addressed SHA-256 before
# changing the live pointer.
set -- "$release_dir"/data/catalog-*.sqlite3
[ "$#" -eq 1 ] && [ -f "$1" ] || fail "release must contain exactly one data/catalog-<sha256>.sqlite3 file"
database_path=$1
database_name=${database_path##*/}

first_unexpected_database=$(find "$release_dir" -type f \( \
    -name '*.db' -o \
    -name '*.sqlite' -o \
    -name '*.sqlite3' \
\) ! -path "$database_path" -print -quit)
[ -z "$first_unexpected_database" ] \
    || fail "release contains an unexpected database artifact: $first_unexpected_database"

printf '%s\n' "$database_name" | grep -Eq '^catalog-[0-9a-f]{64}\.sqlite3$' \
    || fail "public database has a non-content-addressed filename: $database_name"

expected_digest=${database_name#catalog-}
expected_digest=${expected_digest%.sqlite3}
actual_digest=$(sha256sum "$database_path" | awk '{print $1}')
[ "$actual_digest" = "$expected_digest" ] \
    || fail "public database SHA-256 does not match its filename"

database_bytes=$(wc -c < "$database_path" | tr -d '[:space:]')
manifest_path=$release_dir/catalog-manifest.json
grep -Fq '"format": "waajacu-public-catalog-v1"' "$manifest_path" \
    || fail "manifest has an unsupported or malformed format"
grep -Fq "\"path\": \"data/$database_name\"" "$manifest_path" \
    || fail "manifest does not reference the content-addressed database"
grep -Fq "\"sha256\": \"sha256:$expected_digest\"" "$manifest_path" \
    || fail "manifest SHA-256 does not match the database"
grep -Fq "\"bytes\": $database_bytes" "$manifest_path" \
    || fail "manifest byte count does not match the database"

current_link=$deploy_root/current
if [ -e "$current_link" ] || [ -L "$current_link" ]; then
    [ -L "$current_link" ] || fail "refusing to replace non-symlink path: $current_link"
fi

next_link=$deploy_root/.current.next.$$
[ ! -e "$next_link" ] && [ ! -L "$next_link" ] \
    || fail "temporary activation path already exists: $next_link"

cleanup() {
    rm -f -- "$next_link"
}
trap cleanup 0 1 2 15

ln -s "releases/$release_id" "$next_link"

# -T is intentional: it prevents mv from following an existing current
# symlink as though it were a directory. Both directory entries are beneath
# DEPLOY_ROOT, so rename(2) performs one atomic pointer switch.
mv -Tf -- "$next_link" "$current_link"

trap - 0 1 2 15
printf 'Activated %s -> releases/%s\n' "$current_link" "$release_id"
