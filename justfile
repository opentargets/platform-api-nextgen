# Developer recipes. Run `just` to list them; see README "Versioning and releases".
set shell := ["bash", "-euo", "pipefail", "-c"]

current := `cargo pkgid | sed 's/.*[#@]//'`
branch  := `git rev-parse --abbrev-ref HEAD`

# List recipes
default:
    @just --list

# Install the dev tools used by this repo (needs uv and cargo)
setup:
    uv tool install pre-commit
    cargo install cargo-release --locked
    pre-commit install

# Print the version in Cargo.toml
version:
    @echo {{current}}

[private]
release-preflight:
    #!/usr/bin/env bash
    set -euo pipefail
    command -v cargo-release >/dev/null || { echo "cargo-release missing: run 'just setup'"; exit 1; }
    [ "{{branch}}" = main ] || { echo "Must be on main (on {{branch}})"; exit 1; }
    git diff --quiet && git diff --cached --quiet || { echo "Working tree not clean"; exit 1; }
    git fetch -q origin main
    [ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] || { echo "main is not in sync with origin/main"; exit 1; }

# Cut the next RC: 26.12.0-dev -> 26.12.0-rc.1, rc.N -> rc.N+1 (dry run unless --execute)
rc *flags: release-preflight
    cargo release rc {{flags}}

# Cut the final release: 26.12.0-dev or -rc.N -> 26.12.0 (dry run unless --execute)
release *flags: release-preflight
    cargo release release {{flags}}

# Start the next cycle, e.g. `just next-version 27.1.0 --execute` sets 27.1.0-dev
next-version next *flags: release-preflight
    #!/usr/bin/env bash
    set -euo pipefail
    cargo release version {{next}}-dev {{flags}}
    if [[ " {{flags}} " == *" --execute "* ]]; then
      git commit -am "chore: start {{next}} development"
      git push origin HEAD
    else
      echo "Dry run: nothing committed or pushed"
    fi

# Build a dev image of the current (pushed) branch, e.g. `just dev-build` or `just dev-build 4628`
dev-build id="":
    #!/usr/bin/env bash
    set -euo pipefail
    git fetch -q origin "{{branch}}"
    [ "$(git rev-parse HEAD)" = "$(git rev-parse "origin/{{branch}}")" ] \
      || { echo "Push {{branch}} first: local HEAD differs from origin"; exit 1; }
    gh workflow run build-dev.yaml --ref "{{branch}}" {{ if id != "" { "-f id=" + id } else { "" } }}
    sleep 3
    gh run list --workflow build-dev.yaml --branch "{{branch}}" --limit 1
