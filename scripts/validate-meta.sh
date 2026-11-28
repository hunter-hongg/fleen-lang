#!/usr/bin/env bash
# validate-meta.sh — website T03: validate release/<version>/meta.json files.
#
# Zero-dependency (bash + python3 stdlib json, no third-party library;
# "JSON schema library" in the ticket card is satisfied by a plain
# structural check over the json module).
#
# Checks per meta.json:
#   1. required fields present with the right types
#   2. `version` matches the directory name
#   3. `status` in {released, implemented, planned}
#   4. `bytecode_version` is a positive integer
#   5. `breaking_changes[]` entries have a non-empty `item` and a `decision_ref`
#   6. a non-empty `decision_ref` must exist as a `### D<n>` heading in
#      docs/<version>/PLAN.md (0.0.1–0.0.4 predate the D-numbering
#      scheme; a missing PLAN.md is allowed for released versions only)
#
# Also checks (D9):
#   7. every version directory under release/ that has a RELEASE_NOTES.md
#      has a meta.json (coverage); 0.0.1–0.0.3 are exempt (T03 scope is
#      0.0.4+)
#
# Exit codes: 0 = all green, 1 = validation failure, 2 = missing/unreadable input.

set -u

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

python3 - "$REPO_ROOT" <<'PY'
import json
import os
import re
import sys

repo = sys.argv[1]
failures = []

STATUS_VALUES = {"released", "implemented", "planned"}
# T03 scope: 0.0.4 and later (0.0.1–0.0.3 are out of scope per the user).
MIN_VERSION = (0, 0, 4)


def parse_version(v):
    parts = v.split(".")
    if len(parts) != 3 or not all(p.isdigit() for p in parts):
        return None
    return tuple(int(p) for p in parts)


def in_scope(version_dir):
    v = parse_version(version_dir)
    return v is not None and v >= MIN_VERSION


release_root = os.path.join(repo, "release")
if not os.path.isdir(release_root):
    print(f"FAIL: release/ directory not found at {release_root}")
    sys.exit(2)

decision_cache = {}


def plan_decisions(version_dir):
    """Return the set of D-number headings in docs/<version>/PLAN.md, or None."""
    if version_dir in decision_cache:
        return decision_cache[version_dir]
    plan = os.path.join(repo, "docs", version_dir, "PLAN.md")
    if not os.path.isfile(plan):
        decision_cache[version_dir] = None
        return None
    with open(plan, encoding="utf-8") as f:
        text = f.read()
    decisions = set(re.findall(r"^### (D\d+) —", text, flags=re.M))
    decision_cache[version_dir] = decisions
    return decisions


required_fields = {
    "version": str,
    "codename": str,
    "emoji": str,
    "status": str,
    "bytecode_version": int,
    "breaking_changes": list,
}

checked = 0
for entry in sorted(os.listdir(release_root)):
    dir_path = os.path.join(release_root, entry)
    if not os.path.isdir(dir_path):
        continue
    if not in_scope(entry):
        continue
    notes = os.path.join(dir_path, "RELEASE_NOTES.md")
    if not os.path.isfile(notes):
        # a version dir without release notes is not a real release; skip
        continue

    checked += 1
    meta_path = os.path.join(dir_path, "meta.json")
    if not os.path.isfile(meta_path):
        failures.append(f"{entry}: meta.json is missing (coverage check)")
        continue
    try:
        with open(meta_path, encoding="utf-8") as f:
            meta = json.load(f)
    except (json.JSONDecodeError, OSError) as e:
        failures.append(f"{entry}: meta.json unreadable: {e}")
        continue

    # 1. required fields
    for field, typ in required_fields.items():
        if field not in meta:
            failures.append(f"{entry}: missing field `{field}`")
        elif typ is int:
            if not isinstance(meta[field], int) or isinstance(meta[field], bool):
                failures.append(f"{entry}: `{field}` must be an integer")
        elif not isinstance(meta[field], typ):
            failures.append(f"{entry}: `{field}` must be a {typ.__name__}")

    # 2. version matches directory
    if "version" in meta and isinstance(meta["version"], str):
        if meta["version"] != entry:
            failures.append(
                f"{entry}: `version` field is `{meta['version']}`, "
                f"expected `{entry}`"
            )

    # 3. status enum
    status = meta.get("status")
    if isinstance(status, str) and status not in STATUS_VALUES:
        failures.append(
            f"{entry}: `status` is `{status}`, "
            f"expected one of {sorted(STATUS_VALUES)}"
        )

    # 4. bytecode_version positive int
    bv = meta.get("bytecode_version")
    if isinstance(bv, int) and not isinstance(bv, bool) and bv < 1:
        failures.append(f"{entry}: `bytecode_version` must be >= 1 (got {bv})")

    # 5 + 6. breaking_changes structure + decision_ref existence
    for i, bc in enumerate(meta.get("breaking_changes") or []):
        label = f"{entry}: breaking_changes[{i}]"
        if not isinstance(bc, dict):
            failures.append(f"{label}: entry must be an object")
            continue
        item = bc.get("item")
        if not isinstance(item, str) or not item.strip():
            failures.append(f"{label}: `item` must be a non-empty string")
        ref = bc.get("decision_ref")
        if not isinstance(ref, str):
            failures.append(f"{label}: `decision_ref` must be a string (use \"\" if none)")
            continue
        if ref:
            m = re.fullmatch(r"D\d+", ref)
            if not m:
                failures.append(
                    f"{label}: `decision_ref` is `{ref}`, "
                    f"expected a `D<n>` heading like `D1`"
                )
                continue
            decisions = plan_decisions(entry)
            if decisions is None:
                failures.append(
                    f"{label}: `decision_ref` `{ref}` but "
                    f"docs/{entry}/PLAN.md does not exist"
                )
            elif ref not in decisions:
                failures.append(
                    f"{label}: `decision_ref` `{ref}` is not a "
                    f"`### {ref} —` heading in docs/{entry}/PLAN.md "
                    f"(available: {sorted(decisions)})"
                )

print(f"checked {checked} meta.json file(s) under release/ (0.0.4+):")
if failures:
    for f_ in failures:
        print(f"  FAIL: {f_}")
    print(f"\n{len(failures)} failure(s)")
    sys.exit(1)
print("  all meta.json files valid")
sys.exit(0)
PY
