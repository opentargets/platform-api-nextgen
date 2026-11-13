#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
temporary_dir=$(mktemp -d)
trap 'rm -rf "$temporary_dir"' EXIT

# Use .graphql extensions for Inspector's SDL file loader.
if git cat-file -e HEAD:graphql.schema 2>/dev/null; then
    git show HEAD:graphql.schema > "$temporary_dir/old.graphql"
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

if ! cmp -s graphql.schema "$temporary_dir/new.graphql"; then
    cp "$temporary_dir/new.graphql" graphql.schema
    echo 'Updated graphql.schema. Review it, git add graphql.schema, and retry the commit.'
    exit 1
fi
