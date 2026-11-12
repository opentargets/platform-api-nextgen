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

# Compare against graphql.schema in master if it exists, otherwise use the deployed schema as a baseline.
export SCHEMA_BASE_DESCRIPTION="Baseline commit: \`$SCHEMA_BASE_SHA\`"
if git cat-file -e "$SCHEMA_BASE_SHA:graphql.schema" 2>/dev/null; then
    git show "$SCHEMA_BASE_SHA:graphql.schema" > "$temporary_dir/old.graphql"
else
    schema_url=https://api.platform.opentargets.xyz/latest/graphql/schema
    echo "Baseline $SCHEMA_BASE_SHA has no graphql.schema; using the deployed schema for bootstrap."
    curl --fail --location "$schema_url" --output "$temporary_dir/old.graphql"
    SCHEMA_BASE_DESCRIPTION="Bootstrap baseline: [$schema_url]($schema_url), fetched $(date -u +%Y-%m-%dT%H:%M:%SZ). Commit \`$SCHEMA_BASE_SHA\` has no schema snapshot; this comparison is against the live deployment, not that commit."
fi
cargo run --quiet --locked --example export-schema --features product-platform \
    > "$temporary_dir/new.graphql"

# Inspector does not invoke onComplete when there are no changes.
printf '# GraphQL schema changes\n\n%s\n\nCompared commit: `%s`\n\nNo schema changes.\n' \
    "$SCHEMA_BASE_DESCRIPTION" "$SCHEMA_HEAD_SHA" > "$SCHEMA_NOTES_PATH"
graphql-inspector diff "$temporary_dir/old.graphql" "$temporary_dir/new.graphql" \
    --onComplete scripts/graphql-schema-notes.cjs
mv "$SCHEMA_NOTES_PATH" "$notes_output"

if ! cmp -s graphql.schema "$temporary_dir/new.graphql"; then
    echo 'graphql.schema does not match the Rust schema. Run the pre-commit hook and commit the updated snapshot.' >&2
    exit 1
fi
