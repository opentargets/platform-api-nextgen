#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
temporary_dir=$(mktemp -d)
trap 'rm -rf "$temporary_dir"' EXIT

# Use .graphql extensions for Inspector's SDL file loader.
if git cat-file -e HEAD:schema.graphql 2>/dev/null; then
    git show HEAD:schema.graphql > "$temporary_dir/old.graphql"
else
    curl --fail --location \
        https://api.platform.opentargets.xyz/latest/graphql/schema \
        --output "$temporary_dir/old.graphql"
fi

# pre-commit temporarily hides unstaged tracked edits during commits.
cargo run --quiet --locked --example export-schema --features product-platform \
    > "$temporary_dir/new.graphql"

graphql-inspector diff "$temporary_dir/old.graphql" "$temporary_dir/new.graphql" \
    --onComplete scripts/graphql-schema-feedback.cjs

if ! cmp -s schema.graphql "$temporary_dir/new.graphql"; then
    cp "$temporary_dir/new.graphql" schema.graphql
    echo 'Updated schema.graphql. Review it, git add schema.graphql, and retry the commit.'
    exit 1
fi
