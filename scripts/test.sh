#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
root=$PWD
export XDG_CACHE_HOME=$root/.cache
mkdir -p "$XDG_CACHE_HOME"
cd tree-sitter-ink
tree-sitter test
for fixture in test/fixtures/*.ink; do
    tree-sitter parse "$fixture" > /dev/null
done
for query in "$root"/languages/ink/*.scm; do
    tree-sitter query "$query" test/fixtures/features.ink > /dev/null
done
tree-sitter fuzz --iterations 25 --edits 3
cd "$root"
cargo test --locked -p ink-lsp
