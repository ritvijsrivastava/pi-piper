#!/bin/sh
# Fails unless the Cargo.toml version matches the root package.json and the
# pi-piper-agent package.json. Releases tag all three together, so they must
# never drift apart. Run in CI (ci.yml) and before publishing (release.yml).
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
cargo_version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$root/Cargo.toml" | head -1)

if [ -z "$cargo_version" ]; then
    echo "error: could not read version from Cargo.toml" >&2
    exit 1
fi

status=0
for file in package.json pi-piper-agent/package.json; do
    json_version=$(node -pe "require('$root/$file').version" 2>/dev/null || true)
    if [ "$cargo_version" != "$json_version" ]; then
        echo "version mismatch: Cargo.toml=$cargo_version $file=${json_version:-<unreadable>}" >&2
        status=1
    fi
done

if [ "$status" -eq 0 ]; then
    echo "all versions agree: $cargo_version"
fi
exit $status
