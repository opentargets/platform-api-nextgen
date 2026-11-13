#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
baseline=${1:?Usage: check-graphql-schema-ci.sh BASE_REF OUTPUT_MARKDOWN}
notes_output=${2:?Provide an output Markdown path}
export SCHEMA_BASE_SHA
export SCHEMA_HEAD_SHA
SCHEMA_BASE_SHA=$(git rev-parse --verify "${baseline}^{commit}")
SCHEMA_HEAD_SHA=$(git rev-parse HEAD)
repository_url="${GITHUB_SERVER_URL:-https://github.com}/${GITHUB_REPOSITORY:-opentargets/platform-api-nextgen}"
base_commit_link="[$SCHEMA_BASE_SHA]($repository_url/commit/$SCHEMA_BASE_SHA)"
export SCHEMA_HEAD_DESCRIPTION="Compared commit: [$SCHEMA_HEAD_SHA]($repository_url/commit/$SCHEMA_HEAD_SHA)"
temporary_dir=$(mktemp -d)
trap 'rm -rf "$temporary_dir"' EXIT
export SCHEMA_NOTES_PATH="$temporary_dir/notes.md"

# Use the baseline commit's graphql.schema, falling back to the deployed schema.
export SCHEMA_BASE_DESCRIPTION="Main comparison commit: $base_commit_link"
if git cat-file -e "$SCHEMA_BASE_SHA:graphql.schema" 2>/dev/null; then
    git show "$SCHEMA_BASE_SHA:graphql.schema" > "$temporary_dir/old.graphql"
else
    schema_url=https://api.platform.opentargets.xyz/latest/graphql/schema
    echo "Baseline $SCHEMA_BASE_SHA has no graphql.schema; using the deployed schema for bootstrap."
    curl --fail --location "$schema_url" --output "$temporary_dir/old.graphql"
    SCHEMA_BASE_DESCRIPTION="Bootstrap baseline: [$schema_url]($schema_url), fetched $(date -u +%Y-%m-%dT%H:%M:%SZ). Main comparison commit $base_commit_link has no schema snapshot. Schema comparison uses the live deployment."
fi
cargo run --quiet --locked --example export-schema --features product-platform \
    > "$temporary_dir/new.graphql"

# Inspector does not invoke onComplete when there are no changes.
printf '# GraphQL schema changes\n\n%s\n\n%s\n\nNo schema changes.\n' \
    "$SCHEMA_BASE_DESCRIPTION" "$SCHEMA_HEAD_DESCRIPTION" > "$SCHEMA_NOTES_PATH"
graphql-inspector diff "$temporary_dir/old.graphql" "$temporary_dir/new.graphql" \
    --onComplete scripts/graphql-schema-notes.cjs
mv "$SCHEMA_NOTES_PATH" "$notes_output"
