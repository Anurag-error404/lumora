#!/usr/bin/env bash
# Bump LUMORA version across package.json, tauri.conf.json, Cargo.toml, and Cargo.lock.
# Usage: bump-version.sh [auto|patch|minor|major]
# Writes version/released/previous_tag to GITHUB_OUTPUT when that env var is set.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

BUMP="${1:-auto}"

current="$(python3 -c "import json; print(json.load(open('package.json'))['version'])")"
IFS=. read -r major minor patch <<<"$current"

last_tag="$(git describe --tags --abbrev=0 --match 'v*' 2>/dev/null || true)"

# No commits since last release → nothing to ship.
if [[ -n "$last_tag" ]]; then
  commits="$(git rev-list "${last_tag}..HEAD" --count)"
  if [[ "$commits" -eq 0 ]]; then
    echo "No commits since ${last_tag}; skipping release."
    if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
      {
        echo "version=${current}"
        echo "released=false"
      } >>"$GITHUB_OUTPUT"
    fi
    exit 0
  fi
fi

# If package.json is already ahead of the latest tag, release that version as-is.
if [[ -n "$last_tag" ]]; then
  tagged="${last_tag#v}"
  if [[ "$current" != "$tagged" ]]; then
    higher="$(
      CURRENT="$current" TAGGED="$tagged" python3 - <<'PY'
import os
current = tuple(int(p) for p in os.environ["CURRENT"].split("."))
tagged = tuple(int(p) for p in os.environ["TAGGED"].split("."))
print("yes" if current > tagged else "no")
PY
    )"
    if [[ "$higher" == "yes" ]]; then
      echo "package.json (${current}) is ahead of ${last_tag}; releasing as-is."
      if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
        {
          echo "version=${current}"
          echo "released=true"
          echo "previous_tag=${last_tag}"
        } >>"$GITHUB_OUTPUT"
      fi
      exit 0
    fi
  fi
fi

if [[ "$BUMP" == "auto" ]]; then
  range="HEAD"
  if [[ -n "$last_tag" ]]; then
    range="${last_tag}..HEAD"
  fi
  log="$(git log "$range" --pretty=%s)"
  if echo "$log" | grep -qE 'BREAKING CHANGE|^[a-z]+(\([^)]*\))?!:'; then
    BUMP="major"
  elif echo "$log" | grep -qiE '^feat(\(|:| )'; then
    BUMP="minor"
  else
    BUMP="patch"
  fi
fi

case "$BUMP" in
  major) major=$((major + 1)); minor=0; patch=0 ;;
  minor) minor=$((minor + 1)); patch=0 ;;
  patch) patch=$((patch + 1)) ;;
  *)
    echo "Unknown bump type: $BUMP" >&2
    exit 1
    ;;
esac

next="${major}.${minor}.${patch}"
echo "Bumping ${current} → ${next} (${BUMP})"

CURRENT="$current" NEXT="$next" python3 - <<'PY'
import json
import os
from pathlib import Path

current = os.environ["CURRENT"]
next_version = os.environ["NEXT"]
root = Path(".")

pkg = json.loads(root.joinpath("package.json").read_text())
pkg["version"] = next_version
root.joinpath("package.json").write_text(json.dumps(pkg, indent=2) + "\n")

tauri = json.loads(root.joinpath("src-tauri/tauri.conf.json").read_text())
tauri["version"] = next_version
root.joinpath("src-tauri/tauri.conf.json").write_text(json.dumps(tauri, indent=2) + "\n")

cargo = root.joinpath("src-tauri/Cargo.toml").read_text()
lines = cargo.splitlines(True)
out = []
replaced = False
for line in lines:
    if not replaced and line.startswith("version = "):
        out.append(f'version = "{next_version}"\n')
        replaced = True
    else:
        out.append(line)
root.joinpath("src-tauri/Cargo.toml").write_text("".join(out))

lock_path = root.joinpath("src-tauri/Cargo.lock")
lock = lock_path.read_text()
needle = f'name = "photovault-ai"\nversion = "{current}"'
repl = f'name = "photovault-ai"\nversion = "{next_version}"'
if needle in lock:
    lock_path.write_text(lock.replace(needle, repl, 1))
PY

if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  {
    echo "version=${next}"
    echo "released=true"
    echo "previous_tag=${last_tag}"
    echo "bump=${BUMP}"
  } >>"$GITHUB_OUTPUT"
fi

echo "VERSION=${next}"
