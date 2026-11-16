#!/usr/bin/env bash
# Build the demo wasm, stamp content-hash versions into the demo assets, and
# deploy website/ to Cloudflare Pages.
#
#   tools/deploy.sh            # build + stamp + deploy
#   tools/deploy.sh --no-build # stamp current demo.wasm, deploy (docs-only change)
#
# Version stamps are the first 10 hex digits of the asset's SHA-256, so a
# changed asset always gets a new URL and an unchanged one keeps its URL
# (repeat runs stay idempotent and produce no diff).
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
demo_dir="$root/website/docs-demo"
wasm_built="$root/target/wasm32-unknown-unknown/release/demo_wasm.wasm"

build=1
for arg in "$@"; do
  case "$arg" in
    --no-build) build=0 ;;
    *) echo "unknown argument: $arg" >&2; exit 2 ;;
  esac
done

if [ "$build" = 1 ]; then
  cargo build -p demo-wasm --target wasm32-unknown-unknown --release
  cp "$wasm_built" "$demo_dir/demo.wasm"
fi

hash_of() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1"
  else shasum -a 256 "$1"; fi | cut -c1-10
}

# Rewrite `prefix?v=<old>` to the new hash, in place, only if it differs.
stamp() {
  local file=$1 prefix=$2 hash=$3
  python3 - "$file" "$prefix" "$hash" <<'PY'
import re, sys
path, prefix, new = sys.argv[1], sys.argv[2], sys.argv[3]
text = open(path).read()
pat = re.compile(re.escape(prefix) + r"\?v=[^\"']*")
current = pat.search(text)
if current is None:
    sys.exit(f"deploy: {path}: no {prefix}?v= stamp found")
if current.group(0) == f"{prefix}?v={new}":
    sys.exit(0)
open(path, "w").write(pat.sub(f"{prefix}?v={new}", text, count=1))
print(f"deploy: stamped {prefix}?v={new}")
PY
}

stamp "$demo_dir/demo.js" "demo.wasm" "$(hash_of "$demo_dir/demo.wasm")"
stamp "$demo_dir/index.html" "demo.js" "$(hash_of "$demo_dir/demo.js")"

# The docs pages carry the generated content; CI fails if they are stale, so
# regenerate before shipping rather than deploying an older copy.
python3 "$root/website/docs-src/build.py"

cd "$root"
if command -v wrangler >/dev/null 2>&1; then
  wrangler pages deploy website --project-name myrust --branch main
else
  npx --yes wrangler pages deploy website --project-name myrust --branch main
fi

# The suite runs against the deployment by default. It needs the Mac, so it
# is a manual step rather than part of this script.
echo
echo "Deployed. Check the deployment with:"
echo "  ssh mac 'ego-browser nodejs' < website/docs-demo/tests/functional.mjs"
