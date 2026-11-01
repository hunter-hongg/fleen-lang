#!/usr/bin/env bash
# Package a Fleen release archive for one target triple.
#
# Usage: scripts/package-release.sh <target-triple> <version> <archive-base>
set -euo pipefail

if [ "$#" -ne 3 ]; then
    echo "Usage: $0 <target-triple> <version> <archive-base>" >&2
    exit 2
fi

TARGET="$1"
VERSION="$2"
ARCHIVE_BASE="$3"

REPO_ROOT="$(git rev-parse --show-toplevel)"
STD_VERSION="$(cat "$REPO_ROOT/std/VERSION")"

if [ "$STD_VERSION" != "$VERSION" ]; then
    echo "error: std/VERSION ($STD_VERSION) does not match version ($VERSION)" >&2
    exit 1
fi

STAGING="$(mktemp -d)"
trap 'rm -rf "$STAGING"' EXIT

BINARIES=(fleen-vm fleen-compiler fleen-verify flint-cli)
for bin in "${BINARIES[@]}"; do
    SRC="$REPO_ROOT/target/$TARGET/release/$bin"
    if [ ! -f "$SRC" ]; then
        echo "error: missing binary $SRC" >&2
        exit 1
    fi
    cp "$SRC" "$STAGING/$bin"
done

cp -R "$REPO_ROOT/std" "$STAGING/std"

cat > "$STAGING/README.md" <<EOF
# Fleen $VERSION — $TARGET

This archive contains the Fleen $VERSION toolchain for $TARGET.

## Contents

- \`fleen-vm\` — Fleen virtual machine / one-stop driver (compile, verify, execute a .fln file)
- \`fleen-compiler\` — Fleen compiler (lex, parse, resolve, typeck, lower, codegen)
- \`fleen-verify\` — Fleen static verification tooling
- \`flint-cli\` — Flint package manager (flint new/build/run/test/check)
- \`std/\` — the Fleen standard library (must match the compiler version)
- \`README.md\` — this file

## Install

1. Add the four binaries to your PATH (copy them to a directory already on PATH).
2. Copy \`std/\` to \`\$FLINT_HOME/std/$VERSION/\`:
   \`mkdir -p "\$FLINT_HOME/std/$VERSION" && cp -R std/* "\$FLINT_HOME/std/$VERSION/"\`.
   The default \`\$FLINT_HOME\` is \`~/.flint\`.
   \`std/VERSION\` must equal \`$VERSION\`; a mismatch is a hard error.
EOF

OUTPUT="$REPO_ROOT/target/release-artifacts/$ARCHIVE_BASE.tar.gz"
mkdir -p "$REPO_ROOT/target/release-artifacts"
tar -czf "$OUTPUT" -C "$STAGING" .
echo "$OUTPUT"
