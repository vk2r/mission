#!/usr/bin/env bash
# Publish a version. Forgejo is the only remote this script writes to; its push mirror carries
# every ref to GitHub, where release.yml builds, attests and publishes the Release. Never forces.
#   mise run publish             publish the version in manifest.json
#   mise run publish -- v1.2.0   first set that version in manifest.json, engine/Cargo.toml and
#                                engine/Cargo.lock and commit it, then publish it
# Run it on dev with a clean tree. Order:
#   1. tag vX.Y.Z on the commit to publish, pushed alone to Forgejo (main is untouched, so the
#      mirror cannot carry a main whose engine does not exist yet);
#   2. wait for the Release vX.Y.Z on GitHub with engine, SHA256SUMS and provenance.sigstore.json;
#   3. require a successful release.yml run for that commit, download the three assets to a temporary
#      directory and verify them exactly as the plugin does (`bin/run-engine --verify-only`: checksum plus
#      the build attestation of release.yml at refs/tags/vX.Y.Z, with cosign; no opt-out here);
#   4. only then fast-forward main on Forgejo to that commit (and local main, when it can follow).
# If it runs out of time or the build fails it stops before step 3; run it again to keep waiting
# (the tag is already pushed). The repository URLs are not stored in .git: they are built from
# FORGEJO_URL and GITHUB_URL (the account base, read from .env at the repository root, which git
# ignores) plus the name of the repository folder. GitHub is only read.
# Needs git, curl, jq and cosign. GITHUB_TOKEN (optional) only lifts the API rate limit.
set -euo pipefail

die() { echo "publish: $*" >&2; exit 1; }

root=$(git rev-parse --show-toplevel)
cd "$root"
[[ -f .env ]] || die "$root/.env not found (needs FORGEJO_URL and GITHUB_URL)"
set -a
# shellcheck source=/dev/null
. "$root/.env"
set +a
[[ -n ${FORGEJO_URL:-} ]] || die "FORGEJO_URL is not set in .env"
[[ -n ${GITHUB_URL:-} ]] || die "GITHUB_URL is not set in .env"
project=$(basename "$root")
forgejo="${FORGEJO_URL%/}/$project.git"
github="${GITHUB_URL%/}/$project.git"
# git@github.com:owner -> owner/<project>, for the API (PUBLISH_REPO overrides it, for tests)
repo="${GITHUB_URL#*github.com[:/]}"
repo="${PUBLISH_REPO:-${repo%/}/$project}"

api=${PUBLISH_API_URL:-https://api.github.com}
interval=${PUBLISH_POLL_INTERVAL:-15}
timeout=${PUBLISH_TIMEOUT:-1800}
assets=(engine SHA256SUMS provenance.sigstore.json)
arg=${1:-}

for tool in git curl jq cosign; do command -v "$tool" >/dev/null || die "$tool is required"; done

[[ $(git rev-parse --abbrev-ref HEAD) == dev ]] || die "not on dev (daily work goes on dev; main only moves with published versions)"
{ git diff --quiet && git diff --cached --quiet; } || die "tracked files have uncommitted changes"
[[ $(git ls-files -- bin) == bin/run-engine ]] || die "bin/ must track exactly bin/run-engine (git add it; the engine itself is never committed)"

manifest_version() { jq -r .version manifest.json; }
cargo_version() { sed -n 's/^version = "\(.*\)"$/\1/p' engine/Cargo.toml | head -n1; }

if [[ -n $arg ]]; then
  [[ $arg =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "version must look like v1.2.3"
  version=${arg#v}
  [[ $(manifest_version) == "$version" && $(cargo_version) == "$version" ]] || {
    jq --arg v "$version" '.version = $v' manifest.json >manifest.json.new && mv manifest.json.new manifest.json
    # Only the first version line: the one in [package].
    sed -i '0,/^version = ".*"$/s//version = "'"$version"'"/' engine/Cargo.toml
    cargo update -w --offline -q --manifest-path engine/Cargo.toml
    git add manifest.json engine/Cargo.toml engine/Cargo.lock
    git commit -q -m "🔖 bump version to $version" -- manifest.json engine/Cargo.toml engine/Cargo.lock
    echo "publish: committed version $version"
  }
fi
version=$(manifest_version)
[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "manifest.json version '$version' is not X.Y.Z"
[[ $(cargo_version) == "$version" ]] || die "engine/Cargo.toml version $(cargo_version) differs from manifest.json $version"
tag=v$version
sha=$(git rev-parse HEAD)

# What Forgejo already has: main must be an ancestor of what we publish (fast-forward only).
if git fetch -q "$forgejo" main 2>/dev/null; then
  git merge-base --is-ancestor FETCH_HEAD "$sha" || die "main on Forgejo is not an ancestor of HEAD; merge or rebase it into dev first"
fi

# The tag must be new, or already ours (a rerun after a timeout).
local_tag=$(git rev-parse -q --verify "refs/tags/$tag^{commit}" || true)
[[ -z $local_tag || $local_tag == "$sha" ]] || die "tag $tag already exists on another commit"
for remote in "$github" "$forgejo"; do
  remote_tag=$(git ls-remote "$remote" "refs/tags/$tag^{}" "refs/tags/$tag" | awk '{print $1}' | tail -n1)
  [[ -z $remote_tag || $remote_tag == "$sha" ]] || die "tag $tag already exists on $remote at another commit; publish a new version"
done
forgejo_has=$(git ls-remote "$forgejo" "refs/tags/$tag" | wc -l)

if [[ -z $local_tag ]]; then
  git tag -a "$tag" -m "$tag"
fi
if ((forgejo_has == 0)); then
  echo "publish: pushing tag $tag (${sha:0:7}) to $forgejo; main stays where it is"
  git push "$forgejo" "refs/tags/$tag"
else
  echo "publish: tag $tag is already on Forgejo, waiting for its Release"
fi

retry="run 'mise run publish' again to keep waiting; main on Forgejo was not moved"
echo "publish: waiting up to ${timeout}s for the Release $tag on GitHub ($repo)"
auth=()
[[ -z ${GITHUB_TOKEN:-} ]] || auth=(-H "Authorization: Bearer $GITHUB_TOKEN")
start=$SECONDS
while :; do
  release=""
  if release=$(curl -fsS --connect-timeout 10 --max-time 30 "${auth[@]}" -H "Accept: application/vnd.github+json" "$api/repos/$repo/releases/tags/$tag" 2>/dev/null); then
    missing=$(jq -r --argjson need "$(printf '%s\n' "${assets[@]}" | jq -R . | jq -s .)" \
      'if .draft then "draft" else ($need - [.assets[]? | select(.state == "uploaded") | .name]) | join(", ") end' <<<"$release")
    [[ -n $missing ]] || break
    echo "publish: Release $tag exists but lacks: $missing ($((SECONDS - start))s)"
  else
    # No Release yet: say whether the build already failed.
    if runs=$(curl -fsS --connect-timeout 10 --max-time 30 "${auth[@]}" -H "Accept: application/vnd.github+json" \
        "$api/repos/$repo/actions/runs?head_sha=$sha&event=push" 2>/dev/null); then
      run=$(jq -c '[.workflow_runs[] | select(.path == ".github/workflows/release.yml")] | sort_by(.created_at) | last // empty' <<<"$runs")
      if [[ -n $run && $(jq -r .status <<<"$run") == completed ]]; then
        conclusion=$(jq -r '.conclusion // "unknown"' <<<"$run")
        [[ $conclusion == success ]] || die "release.yml finished with '$conclusion' ($(jq -r .html_url <<<"$run")); main on Forgejo was not moved. Tag $tag is on Forgejo/GitHub: fix it and publish a new version"
      fi
      echo "publish: no Release $tag yet ($((SECONDS - start))s)"
    else
      echo "publish: API request failed, will retry" >&2
    fi
  fi
  ((SECONDS - start < timeout)) || die "the Release $tag did not appear within ${timeout}s; $retry"
  sleep "$interval"
done

# The Release must belong to the commit we are about to publish.
gh_tag=$(git ls-remote "$github" "refs/tags/$tag^{}" "refs/tags/$tag" | awk '{print $1}' | tail -n1)
[[ $gh_tag == "$sha" ]] || die "tag $tag on GitHub points at '${gh_tag:-nothing}', not ${sha:0:7}; main on Forgejo was not moved"

# The Release exists; now prove it is the real one before main moves. A Release that has the three names
# is not enough: a successful release.yml run for this commit, and checksum + attestation on the assets.
runs=$(curl -fsS --connect-timeout 10 --max-time 30 "${auth[@]}" -H "Accept: application/vnd.github+json" \
  "$api/repos/$repo/actions/runs?head_sha=$sha&event=push" 2>/dev/null) || die "cannot read the workflow runs of $repo; main on Forgejo was not moved; $retry"
[[ $(jq -r '[.workflow_runs[]? | select(.path == ".github/workflows/release.yml" and .head_sha == "'"$sha"'" and .status == "completed" and .conclusion == "success")] | length' <<<"$runs") -ge 1 ]] \
  || die "no successful release.yml run for ${sha:0:7} on $repo (still running or failed); main on Forgejo was not moved; check the run, then run 'mise run publish' again"
verify_dir=$(mktemp -d)
trap 'rm -rf "$verify_dir"' EXIT
env -u MISSION_SKIP_ATTESTATION -u MISSION_TEST -u MISSION_REPO -u MISSION_RELEASE_BASE sh bin/run-engine --verify-only "$verify_dir" >&2 \
  || die "the Release $tag failed checksum or attestation verification; main on Forgejo was not moved"
echo "publish: Release $tag verified (checksum and attestation)"

echo "publish: Release $tag is ready; pushing main (${sha:0:7}) to $forgejo"
git push "$forgejo" "$sha:refs/heads/main"
# Local main follows when it can; otherwise leave it and say so.
if git merge-base --is-ancestor main "$sha" 2>/dev/null; then
  git branch -f main "$sha"
elif ! git rev-parse -q --verify refs/heads/main >/dev/null; then
  git branch main "$sha"
else
  echo "publish: local main is not an ancestor of $tag; left alone"
fi
echo "publish: done; Forgejo's mirror carries main to GitHub"
echo "release: https://github.com/$repo/releases/tag/$tag"
