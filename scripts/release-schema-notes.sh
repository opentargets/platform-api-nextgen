#!/usr/bin/env bash
set -euo pipefail

previous_tag=${1:?Provide the previous release tag}
current_tag=${2:?Provide the current release tag}
notes_output=${3:?Provide the output Markdown path}
temporary_dir=$(mktemp -d)
trap 'rm -rf "$temporary_dir"' EXIT
export SCHEMA_NOTES_PATH="$temporary_dir/notes.md"
repository_url="${GITHUB_SERVER_URL:-https://github.com}/${GITHUB_REPOSITORY:-opentargets/platform-api-nextgen}"

extract_schema() {
    local commit=$1 destination=$2
    for filename in schema.graphql graphql.schema; do
        if git cat-file -e "$commit:$filename" 2>/dev/null; then
            git show "$commit:$filename" > "$destination"
            return
        fi
    done
    echo "Release commit $commit has no saved schema; historical comparison unavailable." >&2
    return 1
}

previous_sha=$(git rev-parse --verify "refs/tags/${previous_tag}^{commit}")
current_sha=$(git rev-parse --verify "refs/tags/${current_tag}^{commit}")
export SCHEMA_BASE_DESCRIPTION="Previous release commit: [$previous_sha]($repository_url/commit/$previous_sha)"
export SCHEMA_HEAD_DESCRIPTION="Current release commit: [$current_sha]($repository_url/commit/$current_sha)"
if ! extract_schema "$previous_sha" "$temporary_dir/old.graphql" ||
   ! extract_schema "$current_sha" "$temporary_dir/new.graphql"; then
    printf '# GraphQL schema changes\n\n%s\n\n%s\n\nSchema comparison unavailable: a release commit has no saved schema.\n' \
        "$SCHEMA_BASE_DESCRIPTION" "$SCHEMA_HEAD_DESCRIPTION" > "$notes_output"
    exit 0
fi
printf '# GraphQL schema changes\n\n%s\n\n%s\n\nNo schema changes.\n' \
    "$SCHEMA_BASE_DESCRIPTION" "$SCHEMA_HEAD_DESCRIPTION" > "$SCHEMA_NOTES_PATH"
graphql-inspector diff "$temporary_dir/old.graphql" "$temporary_dir/new.graphql" \
    --onComplete scripts/graphql-schema-notes.cjs
mv "$SCHEMA_NOTES_PATH" "$notes_output"
