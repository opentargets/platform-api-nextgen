#!/usr/bin/env bash
# Build an empty ClickHouse database with the tables a pos release load creates.
#
# Usage: scripts/clickhouse-from-pos.sh <pos-version> <product> [clickhouse-url]
#
# Creates database <product><YYMM> (YYMM from <pos-version>, e.g. 26.12.0rc17 -> platform2612)
# from the pos image: the landing tables pos generates from the data models (`pos-ddl`), member
# inserts and drops, the declared tables and dictionaries (`tables:`), and the serving scripts, run in the load step's `requires:` order as
# pos/tests/test_serving_sql.py does. Every table is empty, plus the marker table _live_test_marker that the fixture tests require.
# Without a URL, starts clickhouse/clickhouse-server in docker (container clickhouse-pos-test,
# http://localhost:8123) and replaces the database; with a URL, refuses an existing database.
set -euo pipefail

if [[ $# -lt 2 || $# -gt 3 ]]; then
    echo "usage: $0 <pos-version> <product> [clickhouse-url]" >&2
    exit 2
fi
pos_version=$1
product=$2
url=${3:-}
case $product in
platform | ppp) ;;
*) echo "product must be platform or ppp, not $product" >&2; exit 2 ;;
esac

pos_image=europe-west1-docker.pkg.dev/open-targets-eu-dev/pipeline/pos:$pos_version
clickhouse_image=clickhouse/clickhouse-server:26.8.2.7
container=clickhouse-pos-test
IFS=. read -r yy mm _ <<<"$pos_version"
database=$product$yy$mm

run_sql() {
    local out
    if ! out=$(curl -sS --fail-with-body "$url" --data-binary "$1" 2>&1); then
        printf 'statement failed:\n%s\n%s\n' "$1" "$out" >&2
        return 1
    fi
    printf '%s' "$out"
}

replace=0
if [[ -z $url ]]; then
    url=http://localhost:8123
    replace=1
    if [[ -z $(docker ps -q --filter "name=^${container}$") ]]; then
        docker rm -f "$container" >/dev/null 2>&1 || true
        docker run -d --name "$container" -p 8123:8123 -e CLICKHOUSE_SKIP_USER_SETUP=1 \
            --ulimit nofile=262144:262144 "$clickhouse_image" >/dev/null
    fi
    for _ in $(seq 60); do
        curl -sf "$url/ping" >/dev/null && break
        sleep 1
    done
fi
url=${url%/}/

exists=$(run_sql "SELECT count() FROM system.databases WHERE name = '$database'")
if [[ $exists != 0* ]]; then
    if [[ $replace == 1 ]]; then
        run_sql "DROP DATABASE $database SYNC" >/dev/null
    else
        echo "database $database already exists at $url; drop it first" >&2
        exit 1
    fi
fi
run_sql "CREATE DATABASE $database" >/dev/null
url="${url}?database=$database"
# The live fixture tests refuse to TRUNCATE a database without this table.
run_sql "CREATE TABLE _live_test_marker (x UInt8) ENGINE = Memory" >/dev/null

# Prints the statements, NUL-separated.
generate=$(
    cat <<'PY'
import sys
from graphlib import TopologicalSorter
from pathlib import Path

import yaml

from pos.clickhouse_ddl import ddl_for, members
from pos.sql import split_statements
from pos.tasks.clickhouse_load import member_drops, union_inserts

try:
    from pos.clickhouse_ddl import declared_sql
except ImportError:  # images before declared tables
    def declared_sql(entry, product):
        return []

product = sys.argv[1]
step = yaml.safe_load(Path('config/config.yaml').read_text())['steps']['backend_clickhouse']
entries = yaml.safe_load(Path('config/datasets.yaml').read_text())['clickhouse']
tasks = {}
after = {}  # an explode is done once all its children are
for task in step:
    if product not in task.get('products', [product]):
        continue
    for each in task.get('foreach', []):
        for child in task['do']:
            child = yaml.safe_load(yaml.safe_dump(child).replace('${each}', each))
            tasks[child['name']] = child | {'requires': task.get('requires', [])}
            after.setdefault(task['name'], []).append(child['name'])
    if 'foreach' not in task:
        tasks[task['name']] = task
order = TopologicalSorter({n: t.get('requires', []) for n, t in tasks.items()} | after).static_order()
statements = []
for name in order:
    if 'dataset' not in tasks.get(name, {}):
        continue
    entry = entries[tasks[name]['dataset']]
    statements += [sql for _, sql in ddl_for(entry, product)]
    if 'models' in entry:
        loaded = members(entry['models'], product)
        statements += union_inserts(entry['table'], loaded) + member_drops(loaded)
    statements += declared_sql(entry, product)
    for key in ('preload_script', 'postload_script'):
        if script := entry.get(key):
            statements += split_statements(Path(script).read_text())
sys.stdout.write(''.join(f'{s}\0' for s in statements))
PY
)

statements=$(mktemp)
trap 'rm -f "$statements"' EXIT
docker run --rm -i --entrypoint /app/.venv/bin/python "$pos_image" - "$product" \
    <<<"$generate" >"$statements"

count=0
while IFS= read -r -d '' statement; do
    run_sql "$statement" >/dev/null
    count=$((count + 1))
done <"$statements"

tables=$(run_sql "SELECT count() FROM system.tables WHERE database = '$database'")
echo "$database: $count statements, $tables tables at ${url%%\?*}"
