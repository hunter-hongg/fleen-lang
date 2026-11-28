#!/usr/bin/env bash
# check-versions.sh — the single source of truth for "version facts" (website T01).
#
# Asserts that the released version string agrees across the five places the
# repo keeps it. Fails loudly naming *which two* disagree (SPEC §4: actionable
# errors). Zero dependencies beyond bash/grep/sort/awk.
#
# Sources (all read from the repo, none hand-written):
#   1. Cargo.toml   [workspace.package] version = "X.Y.Z"
#   2. std/VERSION  first line, trimmed (file has NO trailing newline)
#   3. CHANGELOG.md first non-[Unreleased] "## [X.Y.Z]" heading
#   4. release/     semver-max directory name
#   5. lang_docs/   semver-max directory name
#
# Run: bash scripts/check-versions.sh
# Exit: 0 all agree, 1 on mismatch, 2 on a source that cannot be read.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

failures=0

# --- semver key: turn X.Y.Z into a sortable numeric triple (avoid '0.0.9' > '0.0.10') ---
semver_key() {
    awk -F. '{ printf "%06d.%06d.%06d\n", $1, $2, $3 }'
}
semver_max() { # max of a list of X.Y.Z strings on stdin
    sort -t. -k1,1n -k2,2n -k3,3n | tail -n1
}

read_version() { # $1 = label, $2 = value ; echoes value, increments failures on empty
    if [ -z "${2:-}" ]; then
        echo "ERROR: could not read version from $1" >&2
        failures=$((failures + 1))
        echo ""
    else
        echo "$2"
    fi
}

# 1. Cargo.toml
cargo_ver="$(grep -m1 -E '^version = "' Cargo.toml | sed -E 's/^version = "([^"]+)".*/\1/')"
v_cargo="$(read_version "Cargo.toml" "$cargo_ver")"

# 2. std/VERSION (trim CR/LF/spaces — file has no trailing newline)
std_ver="$(tr -d '[:space:]' < std/VERSION)"
v_std="$(read_version "std/VERSION" "$std_ver")"

# 3. CHANGELOG.md — skip [Unreleased], take first real "[X.Y.Z]"
chg_ver="$(grep -m1 -E '^## \[[0-9]+\.[0-9]+\.[0-9]+\]' CHANGELOG.md \
    | sed -E 's/^## \[([0-9]+\.[0-9]+\.[0-9]+)\].*/\1/')"
v_changelog="$(read_version "CHANGELOG.md" "$chg_ver")"

# 4. release/ max dir
rel_ver="$(ls -1 release/ 2>/dev/null | grep -E '^[0-9]+\.[0-9]+\.[0-9]+$' | semver_max)"
v_release="$(read_version "release/" "$rel_ver")"

# 5. lang_docs/ max dir
docs_ver="$(ls -1 lang_docs/ 2>/dev/null | grep -E '^[0-9]+\.[0-9]+\.[0-9]+$' | semver_max)"
v_langdocs="$(read_version "lang_docs/" "$docs_ver")"

[ "$failures" -gt 0 ] && exit 2

# --- pairwise agreement ---
declare -A versions=(
    ["Cargo.toml"]="$v_cargo"
    ["std/VERSION"]="$v_std"
    ["CHANGELOG.md"]="$v_changelog"
    ["release/"]="$v_release"
    ["lang_docs/"]="$v_langdocs"
)

mismatches=0
for place in "${!versions[@]}"; do
    if [ "${versions[$place]}" != "$v_cargo" ]; then
        echo "ERROR: version mismatch — $place has ${versions[$place]}, Cargo.toml has $v_cargo" >&2
        mismatches=$((mismatches + 1))
    fi
done

if [ "$mismatches" -gt 0 ]; then
    echo "" >&2
    echo "The released version must agree in all five places (website T01 / PLAN D9)." >&2
    echo "Fix the locations above, or bump the release: see SPEC §13." >&2
    exit 1
fi

echo "OK: version $v_cargo agrees across Cargo.toml, std/VERSION, CHANGELOG.md, release/, lang_docs/"
