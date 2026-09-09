#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

sdk=${WASI_SDK_PATH:-${HOME}/Library/Application Support/Zed/extensions/build/wasi-sdk}
if [[ $# == 2 && $1 == --wasi-sdk ]]; then
    sdk=$2
elif [[ $# != 0 ]]; then
    echo 'Usage: bash scripts/build_dev.sh [--wasi-sdk /path/to/sdk]' >&2
    exit 1
fi
clang=$sdk/bin/clang
if [[ ! -x $clang ]]; then
    echo 'WASI SDK not found. Set WASI_SDK_PATH or pass --wasi-sdk /path/to/sdk.' >&2
    exit 1
fi

bash scripts/prepare_dev.sh
dev=$PWD/.local/dev
trap 'rm -f "$dev/grammars/ink.wasm.tmp"' EXIT
# Clear host SDK settings only inside the compiler subprocess.
(
    unset CPATH C_INCLUDE_PATH CPLUS_INCLUDE_PATH OBJC_INCLUDE_PATH SDKROOT \
        MACOSX_DEPLOYMENT_TARGET LIBRARY_PATH
    "$clang" -fPIC -shared -Os -Wl,--export=tree_sitter_ink \
        -I "$dev/grammars/ink/src" "$dev/grammars/ink/src/"{parser,scanner}.c \
        -o "$dev/grammars/ink.wasm.tmp"
)
mv "$dev/grammars/ink.wasm.tmp" "$dev/grammars/ink.wasm"
bash scripts/build_native.sh
RUSTC_WRAPPER= cargo build --locked --target wasm32-wasip2
cp target/wasm32-wasip2/debug/zed_ink.wasm "$dev/extension.wasm"
printf 'Built extension. In Zed, install the dev extension from %s\n' "$dev"
printf 'The dev extension automatically uses %s/.local/native/ink-lsp\n' "$PWD"
