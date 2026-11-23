#!/usr/bin/env bash
# Merges upstream/main into the current branch. Run it on a branch and open a
# PR; a push to main publishes a release.
set -euo pipefail
cd "$(dirname "$0")/../.."

if ! git diff --quiet || ! git diff --cached --quiet; then
  echo "Commit or stash your changes first." >&2
  exit 1
fi

git remote get-url upstream >/dev/null 2>&1 ||
  git remote add upstream https://github.com/michellemayes/nootle.git
# Both sides rewrite these on every release, so a three-way merge always
# conflicts. .gitattributes picks this driver; it keeps our side.
git config merge.ours.driver true
git fetch upstream --tags

git merge --no-ff --no-edit upstream/main
node scripts/fork/check-updater-config.mjs
echo "Merged. Review with 'git diff origin/main --stat', then push the branch and open a PR."
