#!/usr/bin/env python3
"""Build and stage the native Rust language server for the current host."""
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def build(env=None):
    env = dict(os.environ if env is None else env)
    env['RUSTC_WRAPPER'] = ''
    info = subprocess.check_output(['rustc', '-vV'], text=True, env=env)
    host = next(line.removeprefix('host: ') for line in info.splitlines() if line.startswith('host: '))
    subprocess.run(['cargo', 'build', '--locked', '--release', '-p', 'ink-lsp', '--target', host],
                   cwd=ROOT, env=env, check=True)
    executable = 'ink-lsp.exe' if 'windows' in host else 'ink-lsp'
    destination = ROOT / '.local' / 'native'
    destination.mkdir(parents=True, exist_ok=True)
    shutil.copy2(ROOT / 'target' / host / 'release' / executable, destination / 'ink-lsp')
    (destination / 'host.txt').write_text(host + '\n')
    print(f'Built Rust language server for {host}')


if __name__ == '__main__':
    build()
