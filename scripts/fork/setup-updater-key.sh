#!/usr/bin/env bash
# One-time setup of this fork's updater signing key: the public half goes into
# tauri.conf.json, the private half into the repository's Actions secrets.
# Usage: scripts/fork/setup-updater-key.sh [key-file]
set -euo pipefail
cd "$(dirname "$0")/../.."

KEY_FILE="${1:-$HOME/.tauri/nootle-fork.key}"
# Not `gh`'s default repo: with an upstream remote it resolves to upstream.
REPO=$(git remote get-url origin | sed -E 's#^.*github\.com[:/]##; s#\.git$##')

if [ -e "$KEY_FILE" ]; then
  echo "$KEY_FILE already exists; refusing to overwrite a key that signs shipped updates." >&2
  exit 1
fi
mkdir -p "$(dirname "$KEY_FILE")"

read -rsp "Key password (empty for none): " PASSWORD
echo
pnpm tauri signer generate --ci -p "$PASSWORD" -w "$KEY_FILE" >/dev/null

PUBKEY=$(cat "$KEY_FILE.pub")
sed -i.bak -E "s|(\"pubkey\": \")[^\"]*|\1$PUBKEY|" src-tauri/tauri.conf.json
rm src-tauri/tauri.conf.json.bak

gh secret set TAURI_SIGNING_PRIVATE_KEY -R "$REPO" < "$KEY_FILE"
if [ -n "$PASSWORD" ]; then
  gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD -R "$REPO" --body "$PASSWORD"
fi

echo "Registered the signing key for $REPO. Back up $KEY_FILE: losing it strands installed copies on their current version."
echo "Commit src-tauri/tauri.conf.json to ship the new public key."
