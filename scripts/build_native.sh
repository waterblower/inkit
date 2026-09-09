#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

host=$(rustc -vV | sed -n 's/^host: //p')
RUSTC_WRAPPER= cargo build --locked --release -p ink-lsp --target "$host"
executable=ink-lsp
[[ $host != *windows* ]] || executable=ink-lsp.exe
mkdir -p .local/native
cp "target/$host/release/$executable" .local/native/ink-lsp
printf '%s\n' "$host" > .local/native/host.txt
printf '%s/.local/native/ink-lsp\n' "$PWD" > .local/dev-server-path
