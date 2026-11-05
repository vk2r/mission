#!/usr/bin/env bash
# Publish main: push to GitHub, wait for build-engine to commit the signed binary,
# bring that commit back and push the same history to Forgejo. Never forces.
#   mise run publish            publish main
#   mise run publish -- v1.2.3  also create and push that tag after the binary landed
# Needs git, curl and jq. GITHUB_TOKEN (optional) only lifts the API rate limit.
set -euo pipefail

github=${PUBLISH_GITHUB_REMOTE:-github}
forgejo=${PUBLISH_FORGEJO_REMOTE:-forgejo}
repo=${PUBLISH_REPO:-vk2r/mission}
api=${PUBLISH_API_URL:-https://api.github.com}
workflow_path=${PUBLISH_WORKFLOW_PATH:-.github/workflows/build-engine.yml}
interval=${PUBLISH_POLL_INTERVAL:-20}
appear_timeout=${PUBLISH_APPEAR_TIMEOUT:-180}
timeout=${PUBLISH_TIMEOUT:-1200}
tag=${1:-}

# Paths that trigger build-engine.yml (keep in sync with its `on.push.paths`).
build_paths=(engine mise.toml scripts/build-engine.sh "$workflow_path")

die() { echo "publish: $*" >&2; exit 1; }
for tool in git curl jq; do command -v "$tool" >/dev/null || die "$tool is required"; done

[[ $(git rev-parse --abbrev-ref HEAD) == main ]] || die "not on main"
{ git diff --quiet && git diff --cached --quiet; } || die "tracked files have uncommitted changes"
[[ -z $tag || $tag == v* ]] || die "tag must start with v (release.yml listens to v*)"
[[ -z $tag ]] || ! git rev-parse -q --verify "refs/tags/$tag" >/dev/null || die "tag $tag already exists"

# Will this push start a build? Same rule as the workflow's `paths` filter.
expect_build=1
if git fetch -q "$github" main 2>/dev/null; then
  if git diff --quiet "$github/main" HEAD -- "${build_paths[@]}"; then expect_build=0; fi
fi

sha=$(git rev-parse HEAD)
echo "publish: pushing ${sha:0:7} to $github"
git push "$github" main

if ((expect_build)); then
  echo "publish: waiting for build-engine on ${sha:0:7}"
  auth=()
  [[ -z ${GITHUB_TOKEN:-} ]] || auth=(-H "Authorization: Bearer $GITHUB_TOKEN")
  start=$SECONDS
  while :; do
    if runs=$(curl -fsS "${auth[@]}" -H "Accept: application/vnd.github+json" \
        "$api/repos/$repo/actions/runs?head_sha=$sha&event=push"); then
      run=$(jq -c --arg p "$workflow_path" \
        '[.workflow_runs[] | select(.path == $p)] | sort_by(.created_at) | last // empty' <<<"$runs")
    else
      run=""; echo "publish: API request failed, will retry" >&2
    fi
    if [[ -z $run ]]; then
      ((SECONDS - start < appear_timeout)) || die "no build-engine run appeared for ${sha:0:7} after ${appear_timeout}s; $forgejo untouched"
    else
      status=$(jq -r .status <<<"$run")
      if [[ $status == completed ]]; then
        conclusion=$(jq -r '.conclusion // "unknown"' <<<"$run")
        [[ $conclusion == success ]] || die "build-engine finished with '$conclusion' ($(jq -r .html_url <<<"$run")); $forgejo untouched"
        break
      fi
      echo "publish: build-engine is $status ($((SECONDS - start))s)"
    fi
    ((SECONDS - start < timeout)) || die "build-engine did not finish within ${timeout}s; $forgejo untouched"
    sleep "$interval"
  done
  echo "publish: build-engine succeeded"
else
  echo "publish: no build-engine paths changed, not waiting for a build"
fi

git fetch -q "$github" main
git merge --ff-only "$github/main"

if [[ -n $tag ]]; then
  git tag -a "$tag" -m "$tag"
  git push "$github" "refs/tags/$tag"
fi

echo "publish: pushing $(git rev-parse --short HEAD) to $forgejo"
git push "$forgejo" main
[[ -z $tag ]] || git push "$forgejo" "refs/tags/$tag"
echo "publish: done"
