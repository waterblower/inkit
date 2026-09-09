#!/usr/bin/env python3
"""Snapshot the bundled parser in a local Git repo and configure Zed to use it."""
import json
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / 'tree-sitter-ink'
SNAPSHOT = ROOT / '.local' / 'grammar-source'


def git(*args):
    return subprocess.check_output(
        ['git', '-c', 'core.hooksPath=/dev/null', '-c', 'commit.gpgsign=false',
         '-c', 'user.name=Ink extension build', '-c', 'user.email=ink-build@localhost',
         *args], cwd=SNAPSHOT, text=True, stderr=subprocess.PIPE).strip()


def main():
    SNAPSHOT.mkdir(parents=True, exist_ok=True)
    if not (SNAPSHOT / '.git').exists():
        git('init', '--quiet')
    for filename in ['grammar.js', 'tree-sitter.json', 'package.json']:
        shutil.copy2(SOURCE / filename, SNAPSHOT / filename)
    shutil.copytree(SOURCE / 'src', SNAPSHOT / 'src', dirs_exist_ok=True)
    git('add', 'grammar.js', 'tree-sitter.json', 'package.json', 'src')
    if git('status', '--porcelain'):
        git('commit', '--quiet', '-m', 'Snapshot bundled Ink grammar for local Zed development')
    rev = git('rev-parse', 'HEAD')
    # JSON strings are valid TOML basic strings, including escaping of paths.
    manifest = (ROOT / 'extension.toml.in').read_text()
    manifest = manifest.replace('"@GRAMMAR_REPOSITORY@"', json.dumps(SNAPSHOT.as_uri()))
    manifest = manifest.replace('@GRAMMAR_REV@', rev)
    (ROOT / 'extension.toml').write_text(manifest)
    print(f'Prepared Ink grammar {rev[:12]}')
    print(f'In Zed, run "zed: install dev extension" and select {ROOT}')


if __name__ == '__main__':
    main()
