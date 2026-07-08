#!/usr/bin/env bash
# Build the Leptos CSR frontend to a browser-ready bundle WITHOUT Trunk, using
# wasm-bindgen-cli directly. The CLI version must match the `wasm-bindgen` crate
# version (checked below) or the generated glue is incompatible.
set -euo pipefail

FRONTEND="$(cd "$(dirname "$0")/../.." && pwd)/rust-gql-frontend"
cd "$FRONTEND"

DEP_VERSION="$(grep -A1 'name = "wasm-bindgen"$' Cargo.lock | grep version | head -1 | sed 's/.*"\(.*\)".*/\1/')"
CLI_VERSION="$(wasm-bindgen --version | awk '{print $2}')"
if [ "$DEP_VERSION" != "$CLI_VERSION" ]; then
	echo "wasm-bindgen-cli $CLI_VERSION != crate $DEP_VERSION" >&2
	echo "install the match: cargo install wasm-bindgen-cli --version $DEP_VERSION --locked" >&2
	exit 1
fi

# Default is a debug build (includes the console_error_panic_hook for readable
# panics). Pass --release for the lean production bundle (hook compiled out).
PROFILE="debug"
CARGO_FLAG=""
if [ "${1:-}" = "--release" ]; then
	PROFILE="release"
	CARGO_FLAG="--release"
fi

cargo build $CARGO_FLAG --target wasm32-unknown-unknown
wasm-bindgen --target web --no-typescript \
	--out-dir dist \
	"target/wasm32-unknown-unknown/$PROFILE/rust_gql_frontend.wasm"

# Port 5173 matches the backend's default [cors].origins, so the browser fetch
# from the page to :4000 is not blocked.
echo "built dist/ ($PROFILE) — serve it and open in a browser:"
echo "  (cd $FRONTEND && python3 -m http.server 5173)"
echo "  open http://localhost:5173  (backend must be running on :4000)"
