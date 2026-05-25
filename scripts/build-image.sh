#!/usr/bin/env bash
# build-image.sh — build images/factorforth.image from a stock Factor
# bootstrap image plus the FactorForth vocabs in factor/.
#
# This is a one-shot bootstrap step.  The resulting image is loaded at
# runtime by newfactor-ui (which uses the patched factor.dll's embedding
# API).  The stock factor.exe used here is just the image builder — it
# is NOT used at runtime.
#
# Inputs (assumed to exist):
#   E:/factor/factor.exe                  — stock Factor binary
#   E:/factor/factor.image                — its bootstrapped image
#   E:/NewFactor/factor/runtime/*         — forth.runtime vocab
#   E:/NewFactor/factor/wf64-gfx/*        — forth.wf64-gfx vocab
#
# Output:
#   E:/NewFactor/images/factorforth.image
#
# Usage:
#   bash scripts/build-image.sh
#
# What the image contains:
#   - Loaded: forth.runtime, forth.wf64-gfx
#   - init-remote-control invoked (registers the OBJ_EVAL_CALLBACK
#     hook our embedding API needs)
#   - Startup quotation: [ boot do-startup-hooks init-remote-control ]
#     so the same hook re-registers on each load
#
# What it does NOT contain:
#   - Any FFI library registration for "nf-host" — that happens at
#     runtime when newfactor-ui calls nf_host_register_library

set -euo pipefail

FACTOR_EXE="E:/factor/factor.exe"
FACTOR_IMG="E:/factor/factor.image"
NF_ROOT="E:/NewFactor"
NF_FACTOR_ROOT="$NF_ROOT/factor"
OUT_IMAGE="$NF_ROOT/images/factorforth.image"

if [[ ! -x "$FACTOR_EXE" ]]; then
    echo "error: $FACTOR_EXE not found or not executable" >&2
    exit 1
fi
if [[ ! -f "$FACTOR_IMG" ]]; then
    echo "error: $FACTOR_IMG not found" >&2
    exit 1
fi

mkdir -p "$NF_ROOT/images"

# Factor expression to evaluate.  Notes:
#   - `<< ... >>` runs at parse time so add-vocab-root takes effect
#     before USE: tries to resolve.
#   - `init-remote-control` is in `alien.remote-control`; it sets up
#     the OBJ_EVAL_CALLBACK that our embedded VM uses via nf_eval_string.
#   - `save-image-and-exit` writes the current image to disk and exits.
read -r -d '' FACTOR_EXPR <<'EOF' || true
USING: vocabs.loader namespaces init ;
<< "E:/NewFactor/factor" add-vocab-root >>
USING: forth.runtime forth.wf64-gfx alien.remote-control ;
init-remote-control
[ boot do-startup-hooks init-remote-control ] set-startup-quot
"E:/NewFactor/images/factorforth.image" save-image-and-exit
EOF

echo "Building $OUT_IMAGE ..."
echo "----------------------------------------------------------------"

"$FACTOR_EXE" -i="$FACTOR_IMG" -e="$FACTOR_EXPR"

if [[ -f "$OUT_IMAGE" ]]; then
    SIZE=$(stat -c%s "$OUT_IMAGE" 2>/dev/null || stat -f%z "$OUT_IMAGE")
    echo "----------------------------------------------------------------"
    echo "OK: $OUT_IMAGE ($SIZE bytes)"
else
    echo "error: image was not produced" >&2
    exit 1
fi
