#!/usr/bin/env bash
# Build the full documentation site: the mdBook (ES/EN) AND the API reference
# (rustdoc), placed side by side under rust-gql-docs/book/ so the book can link
# code items to their source.
#
#   rust-gql-docs/book/
#   ├── index.html      (language portal)
#   ├── es/  en/        (mdBook)
#   └── rustdoc/        (cargo doc — api, xtask, xcheck, rust-gql-domain)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DOCS="$ROOT/rust-gql-docs"

# 1) mdBook (both languages)
mdbook build "$DOCS/es"
mdbook build "$DOCS/en"

# 2) rustdoc — backend workspace + the shared domain crate, with intra-doc links
#    intact (no deps, so it stays small and fast).
(cd "$ROOT/rust-gql-backend" && cargo doc --no-deps -p api -p xtask -p xcheck -p rust-gql-domain)

# 3) place rustdoc parallel to the book
rm -rf "$DOCS/book/rustdoc"
cp -R "$ROOT/rust-gql-backend/target/doc" "$DOCS/book/rustdoc"

# rustdoc has no root index for a multi-crate set: add a tiny redirect to `api`.
cat > "$DOCS/book/rustdoc/index.html" <<'HTML'
<!doctype html>
<meta charset="utf-8">
<title>API reference (rustdoc)</title>
<meta http-equiv="refresh" content="0; url=api/index.html">
<a href="api/index.html">API reference →</a>
HTML

# Language portal at book/index.html (the whole book/ is .gitignored, so this must
# be regenerated here to stay reproducible).
cat > "$DOCS/book/index.html" <<'HTML'
<!doctype html>
<html lang="en">
	<head>
		<meta charset="utf-8" />
		<meta name="viewport" content="width=device-width, initial-scale=1" />
		<title>Taller GQL — Rust Migration</title>
		<style>
			body { font-family: system-ui, sans-serif; background: #161923; color: #eaecef;
				display: grid; place-items: center; min-height: 100vh; margin: 0; }
			.card { text-align: center; }
			h1 { font-weight: 600; }
			a { display: inline-block; margin: 0.5rem; padding: 0.75rem 1.5rem;
				border: 1px solid #4a5168; border-radius: 8px; color: #eaecef; text-decoration: none; }
			a:hover { background: #2b303b; }
		</style>
	</head>
	<body>
		<div class="card">
			<h1>Taller GQL — Rust Migration</h1>
			<p>Choose language · Elige idioma</p>
			<a href="./en/introduction.html">🇬🇧 English</a>
			<a href="./es/introduction.html">🇪🇸 Español</a>
			<p style="margin-top:1.5rem;opacity:.7;font-size:.9rem;">API reference</p>
			<a href="./rustdoc/index.html">🦀 rustdoc</a>
		</div>
	</body>
</html>
HTML

echo "docs built:"
echo "  book (ES): $DOCS/book/es/index.html"
echo "  book (EN): $DOCS/book/en/index.html"
echo "  rustdoc:   $DOCS/book/rustdoc/index.html  (-> api/index.html)"
