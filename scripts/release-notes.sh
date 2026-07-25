#!/usr/bin/env bash
# release-notes.sh — render the GitHub Release body for a tag, on stdout.
#
# The prose lives in .github/release-notes.md.tmpl; this script only fills its
# placeholders, so changing the wording never means changing shell. The Release
# workflow and `task release-notes` run this same file, so a local preview is
# the real body rather than an approximation of it.
#
#   scripts/release-notes.sh v0.1.0            # preview
#   scripts/release-notes.sh v0.1.0 > NOTES.md # what CI does
#
# REPO_URL is set by the workflow; locally it is derived from the git remote.
set -euo pipefail

tag="${1:-}"
if [ -z "$tag" ]; then
  echo "usage: $0 <tag>   e.g. $0 v0.1.0" >&2
  exit 2
fi
ver="${tag#v}"

root="$(cd "$(dirname "$0")/.." && pwd)"
tmpl="$root/.github/release-notes.md.tmpl"

repo_url="${REPO_URL:-$(git -C "$root" remote get-url origin |
  sed -e 's|^git@github\.com:|https://github.com/|' -e 's|\.git$||')}"

# The Zellij compat floor is derived from the lockfile, never hardcoded.
zellij_tile="$(grep -A2 'name = "zellij-tile"' "$root/Cargo.lock" |
  grep version | head -n1 | cut -d'"' -f2)"

changes="$(mktemp)"
trap 'rm -f "$changes"' EXIT

# Cut the body of this version's section out of CHANGELOG.md: start after the
# `## [<ver>]` heading, stop at whatever ends it — the next `## [` heading, or
# the link-reference definitions git-cliff appends (`[0.1.0]: https://…`).
awk -v ver="$ver" '
  $0 ~ "^## \\[" ver "\\]"          { found = 1; next }
  found && (/^## \[/ || /^\[[^]]+\]: /) { exit }
  found                             { print }
' "$root/CHANGELOG.md" >"$changes"

if [ -z "$(tr -d '[:space:]' <"$changes")" ]; then
  echo "$0: CHANGELOG.md has no section for $ver — run 'task changelog NEW=$ver'" >&2
  exit 1
fi

# `r` queues the file, `d` drops the placeholder line: the classic sed include.
# `|` as the delimiter keeps the URLs slash-safe.
notes="$(sed \
  -e "s|{{ZELLIJ_TILE}}|$zellij_tile|g" \
  -e "s|{{REPO_URL}}|$repo_url|g" \
  -e "s|{{TAG}}|$tag|g" \
  -e "/{{CHANGES}}/r $changes" \
  -e "/{{CHANGES}}/d" \
  "$tmpl")"

# A typo'd placeholder would otherwise ship as literal braces in the Release.
if printf '%s\n' "$notes" | grep -q '{{'; then
  echo "$0: unsubstituted placeholder in the rendered notes:" >&2
  printf '%s\n' "$notes" | grep -n '{{' >&2
  exit 1
fi

printf '%s\n' "$notes"
