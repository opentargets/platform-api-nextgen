#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
temporary_dir=$(mktemp -d)
trap 'rm -rf "$temporary_dir"' EXIT

# pre-commit hides unstaged tracked changes during commits.
# Export must succeed before replacing or staging the generated snapshot.
cargo run --quiet --locked --example export-schema --features product-platform \
    > "$temporary_dir/schema.graphql"
cp "$temporary_dir/schema.graphql" schema.graphql
git add -- schema.graphql
echo 'Generated and staged schema.graphql.'
