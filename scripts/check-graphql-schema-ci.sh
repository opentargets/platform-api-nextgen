#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
baseline=${1:?Usage: check-graphql-schema-ci.sh BASE_REF OUTPUT_MARKDOWN}
notes_output=${2:?Provide an output Markdown path}
SCHEMA_BASE_SHA=$(git rev-parse --verify "${baseline}^{commit}")
SCHEMA_HEAD_SHA=$(git rev-parse HEAD)
repository_url="${GITHUB_SERVER_URL:-https://github.com}/${GITHUB_REPOSITORY:-opentargets/platform-api-nextgen}"
base_commit_link="[$SCHEMA_BASE_SHA]($repository_url/commit/$SCHEMA_BASE_SHA)"
export SCHEMA_HEAD_DESCRIPTION="Compared commit: [$SCHEMA_HEAD_SHA]($repository_url/commit/$SCHEMA_HEAD_SHA)"
temporary_dir=$(mktemp -d)
trap 'rm -rf "$temporary_dir"' EXIT
export SCHEMA_NOTES_PATH="$temporary_dir/notes.md"

export SCHEMA_BASE_DESCRIPTION="Main comparison commit: $base_commit_link"

extract_schema() {
    local commit=$1 destination=$2
    if git cat-file -e "$commit:schema.graphql" 2>/dev/null; then
        git show "$commit:schema.graphql" > "$destination"
        return
    fi
    echo "Commit $commit has no saved schema; schema comparison unavailable." >&2
    return 1
}

if ! extract_schema "$SCHEMA_BASE_SHA" "$temporary_dir/old.graphql" ||
   ! extract_schema "$SCHEMA_HEAD_SHA" "$temporary_dir/new.graphql"; then
    printf '# GraphQL schema changes\n\n%s\n\n%s\n\nSchema comparison unavailable: a compared commit has no saved schema.\n' \
        "$SCHEMA_BASE_DESCRIPTION" "$SCHEMA_HEAD_DESCRIPTION" > "$notes_output"
    exit 0
fi

# Inspector does not invoke onComplete when there are no changes.
printf '# GraphQL schema changes\n\n%s\n\n%s\n\nNo schema changes.\n' \
    "$SCHEMA_BASE_DESCRIPTION" "$SCHEMA_HEAD_DESCRIPTION" > "$SCHEMA_NOTES_PATH"
graphql-inspector diff "$temporary_dir/old.graphql" "$temporary_dir/new.graphql" \
    --onComplete scripts/graphql-schema-notes.cjs
mv "$SCHEMA_NOTES_PATH" "$notes_output"
