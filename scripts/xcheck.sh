#!/usr/bin/env bash
# Cross-contract check (plan Task 12): the SAME `hello` operation, issued by the
# Node client (graphql-codegen) and the Leptos client's request construction
# (graphql_client, via the native `xcheck` bin), must return an identical
# response `data` from the running Rust backend.
#
# Both clients derive from the one emitted schema.graphql, so this is the
# end-to-end proof that GraphQL is a compiler-checked contract shared across the
# wire — not just an API format.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BACKEND="$ROOT/rust-gql-backend"
ENDPOINT="${GQL_ENDPOINT:-http://localhost:4000/graphql}"

# Dummy secrets: `hello` touches no database or storage, so degraded mode is fine.
export GQL_DATABASE__URL="${GQL_DATABASE__URL:-postgres://d:d@localhost/gql}"
export GQL_AUTH__SESSION__SECRET="${GQL_AUTH__SESSION__SECRET:-xcheck-secret-0123456789abcdef-32bytes!!}"
export GQL_STORAGE__URL="${GQL_STORAGE__URL:-https://example.supabase.co}"
export GQL_STORAGE__KEY="${GQL_STORAGE__KEY:-dummy}"

cargo build -q --manifest-path "$BACKEND/Cargo.toml" -p api -p xcheck

RUST_LOG=error "$BACKEND/target/debug/gql-api" >/tmp/xcheck-server.log 2>&1 &
SERVER=$!
trap 'kill "$SERVER" 2>/dev/null || true' EXIT

curl -s --retry 30 --retry-connrefused -o /dev/null -X POST "$ENDPOINT" \
	-H 'content-type: application/json' -d '{"query":"{hello}"}'

NODE_DATA="$(node "$ROOT/gofigeeks-gql-frontend/codegen/xcheck.mjs" "$ENDPOINT")"
LEPTOS_DATA="$("$BACKEND/target/debug/xcheck" "$ENDPOINT")"

echo "node   (graphql-codegen): $NODE_DATA"
echo "leptos (graphql_client) : $LEPTOS_DATA"

if [ "$NODE_DATA" = "$LEPTOS_DATA" ]; then
	echo "PASS: identical response data — cross-contract verified"
else
	echo "FAIL: responses differ" >&2
	exit 1
fi
