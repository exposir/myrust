#!/usr/bin/env bash
# Run the docs-demo functional suite.
#
#   tests/run.sh                    # against https://myrust.pages.dev
#   tests/run.sh http://localhost:18080
#
# `ego-browser nodejs` does not inherit the shell environment, so the base URL
# is spliced into a copy of functional.mjs instead of read from DEMO_BASE.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
base=${1:-https://myrust.pages.dev}

tmp=$(mktemp "${TMPDIR:-/tmp}/docs-demo-suite.XXXXXX.mjs")
trap 'rm -f "$tmp"' EXIT

BASE="$base" python3 - "$here/functional.mjs" "$tmp" <<'PY'
import os, sys
src, dst = sys.argv[1], sys.argv[2]
text = open(src).read()
needle = 'const BASE = "https://myrust.pages.dev";'
if needle not in text:
    sys.exit(f"run.sh: base URL line not found in {src}")
open(dst, "w").write(text.replace(needle, f'const BASE = "{os.environ["BASE"]}";', 1))
PY

if [ "$(uname)" = "Darwin" ]; then
  ego-browser nodejs < "$tmp"
else
  ssh mac 'ego-browser nodejs' < "$tmp"
fi
