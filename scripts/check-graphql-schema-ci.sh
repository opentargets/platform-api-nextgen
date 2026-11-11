#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
baseline=${1:?Usage: check-graphql-schema-ci.sh BASE_REF OUTPUT_MARKDOWN}
notes_output=${2:?Provide an output Markdown path}
export SCHEMA_BASE_SHA
export SCHEMA_HEAD_SHA
SCHEMA_BASE_SHA=$(git rev-parse --verify "${baseline}^{commit}")
SCHEMA_HEAD_SHA=$(git rev-parse HEAD)
temporary_dir=$(mktemp -d)
trap 'rm -rf "$temporary_dir"' EXIT
export SCHEMA_NOTES_PATH="$temporary_dir/notes.md"

if ! git show "$SCHEMA_BASE_SHA:graphql.schema" > "$temporary_dir/old.graphql"; then
    echo 'The baseline commit needs a committed graphql.schema; no live deployment fallback is used in CI.' >&2
    exit 1
fi
cargo run --quiet --locked --example export-schema --features product-platform \
    > "$temporary_dir/new.graphql"

# Inspector does not invoke onComplete when there are no changes.
printf '# GraphQL schema changes\n\nBaseline commit: `%s`\n\nCompared commit: `%s`\n\nNo schema changes.\n' \
    "$SCHEMA_BASE_SHA" "$SCHEMA_HEAD_SHA" > "$SCHEMA_NOTES_PATH"
graphql-inspector diff "$temporary_dir/old.graphql" "$temporary_dir/new.graphql" \
    --onComplete scripts/graphql-schema-notes.cjs
mv "$SCHEMA_NOTES_PATH" "$notes_output"

if ! cmp -s graphql.schema "$temporary_dir/new.graphql"; then
    echo 'graphql.schema does not match the Rust schema. Run the pre-commit hook and commit the updated snapshot.' >&2
    exit 1
fi
