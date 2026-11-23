// Fails when this fork's updater could still fetch or trust upstream builds.
// Run by the release workflow and by sync-upstream.sh, because merging
// upstream is what silently brings the upstream endpoint and key back.
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

const UPSTREAM_REPO = "michellemayes/nootle";
const UPSTREAM_PUBKEY =
  "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDIzQzc2OTVEM0RFQ0JGODgKUldTSXYrdzlYV25ISXlxaU80Z0NkTVdnL0ZFUWd0VEZaNXd6MVcyM3hDdlhXZVFNWWFUREJQRi8K";

function currentRepo() {
  if (process.env.GITHUB_REPOSITORY) return process.env.GITHUB_REPOSITORY;
  // Not `gh repo view`: with an upstream remote it resolves to upstream.
  const url = execFileSync("git", ["remote", "get-url", "origin"], { encoding: "utf8" });
  return url.trim().replace(/^.*github\.com[:/]/, "").replace(/\.git$/, "");
}

const repo = currentRepo();
if (repo.toLowerCase() === UPSTREAM_REPO) process.exit(0);

const conf = JSON.parse(
  readFileSync(new URL("../../src-tauri/tauri.conf.json", import.meta.url), "utf8"),
);
const { endpoints = [], pubkey } = conf.plugins.updater;
const errors = [];

const prefix = `https://github.com/${repo}/releases/`.toLowerCase();
for (const e of endpoints) {
  if (!e.toLowerCase().startsWith(prefix)) errors.push(`updater endpoint ${e} is not under ${prefix}`);
}
if (endpoints.length === 0) errors.push("updater has no endpoints");
if (pubkey === UPSTREAM_PUBKEY) {
  errors.push("updater pubkey is still upstream's; run scripts/fork/setup-updater-key.sh");
}

for (const e of errors) console.error(`::error title=Updater config::${e}`);
process.exit(errors.length ? 1 : 0);
