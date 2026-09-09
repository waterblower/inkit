#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
root=$PWD
snapshot=$root/.local/grammar-source
mkdir -p "$snapshot"
git_snapshot() {
    git -C "$snapshot" -c core.hooksPath=/dev/null -c commit.gpgsign=false \
        -c user.name='Ink extension build' -c user.email=ink-build@localhost "$@"
}
[[ -d $snapshot/.git ]] || git_snapshot init --quiet
cp tree-sitter-ink/{grammar.js,tree-sitter.json,package.json} "$snapshot/"
cp -R tree-sitter-ink/src "$snapshot/"
git_snapshot add grammar.js tree-sitter.json package.json src
if ! git_snapshot diff --cached --quiet; then
    git_snapshot commit --quiet -m 'Snapshot bundled Ink grammar for local Zed development'
fi
rev=$(git_snapshot rev-parse HEAD)

# Percent-encode paths, including spaces and Unicode, for the local Git URL.
export LC_ALL=C
repository=file://
for ((i=0; i<${#snapshot}; i++)); do
    char=${snapshot:i:1}
    case $char in
        [a-zA-Z0-9/._~-]) repository+=$char ;;
        *) printf -v encoded '%%%02X' "'$char"; repository+=$encoded ;;
    esac
done

# Zed validates this checkout even when the grammar WASM is already built.
dev=$root/.local/dev
mkdir -p "$dev/grammars" "$dev/lsp"
cp Cargo.toml Cargo.lock "$dev/"
cp -R src languages "$dev/"
cp lsp/Cargo.toml lsp/build.rs "$dev/lsp/"
cp -R lsp/src "$dev/lsp/"
checkout=$dev/grammars/ink
if [[ ! -e $checkout ]]; then
    git clone --quiet --no-checkout "$repository" "$checkout"
fi
if [[ -n $(git -C "$checkout" status --porcelain) ]]; then
    echo "Grammar checkout has local edits: $checkout" >&2
    exit 1
fi
origin=$(git -C "$checkout" remote get-url origin)
if [[ $origin != "$repository" ]]; then
    # Repair a moved project's cache only when it contains this exact snapshot.
    if [[ $origin == file://*/.local/grammar-source && $(git -C "$checkout" rev-parse HEAD) == "$rev" ]]; then
        git -C "$checkout" remote set-url origin "$repository"
    else
        echo "Grammar checkout belongs to a different repository: $origin" >&2
        exit 1
    fi
fi
git -C "$checkout" fetch --quiet origin "$rev"
git -C "$checkout" -c core.hooksPath=/dev/null checkout --quiet --detach "$rev"
sed -e "s|@GRAMMAR_REPOSITORY@|$repository|g" -e "s|@GRAMMAR_REV@|$rev|g" \
    extension.toml.in > "$dev/extension.toml"
printf 'Prepared Ink grammar %s\n' "$rev"
