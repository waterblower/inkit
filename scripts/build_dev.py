#!/usr/bin/env python3
"""Build Zed's cached grammar with a clean WebAssembly compiler environment."""
import argparse
import os
from pathlib import Path
import subprocess

import prepare_dev
import build_native
import shutil


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--wasi-sdk', type=Path, help='WASI SDK directory containing bin/clang')
    args = parser.parse_args()
    sdk = args.wasi_sdk or Path(os.environ.get(
        'WASI_SDK_PATH',
        str(Path.home() / 'Library/Application Support/Zed/extensions/build/wasi-sdk')))
    clang = sdk / 'bin' / ('clang.exe' if os.name == 'nt' else 'clang')
    if not clang.is_file():
        parser.error('WASI SDK not found. Set WASI_SDK_PATH or pass --wasi-sdk /path/to/sdk.')

    prepare_dev.main()
    root = prepare_dev.ROOT
    checkout = root / 'grammars' / 'ink'
    repository = prepare_dev.SNAPSHOT.as_uri()
    rev = prepare_dev.git('rev-parse', 'HEAD')
    checkout.parent.mkdir(exist_ok=True)
    if not checkout.exists():
        subprocess.run(['git', 'clone', '--no-checkout', repository, str(checkout)], check=True)
    remote = subprocess.check_output(
        ['git', '-C', str(checkout), 'remote', 'get-url', 'origin'], text=True).strip()
    if remote != repository:
        parser.error(f'{checkout} belongs to a different grammar repository: {remote}')
    if subprocess.check_output(['git', '-C', str(checkout), 'status', '--porcelain'], text=True).strip():
        parser.error(f'{checkout} has local edits; preserve them before rebuilding the generated checkout.')
    subprocess.run(['git', '-C', str(checkout), 'fetch', '--quiet', 'origin', rev], check=True)
    subprocess.run(['git', '-c', 'core.hooksPath=/dev/null', '-C', str(checkout),
                    'checkout', '--quiet', '--detach', rev], check=True)

    # Host SDK headers are incompatible with the wasm32 target. Sanitize only
    # this child process; do not change the user's shell or Zed configuration.
    env = os.environ.copy()
    for name in ['CPATH', 'C_INCLUDE_PATH', 'CPLUS_INCLUDE_PATH', 'OBJC_INCLUDE_PATH',
                 'SDKROOT', 'MACOSX_DEPLOYMENT_TARGET', 'LIBRARY_PATH']:
        env.pop(name, None)
    source = checkout / 'src'
    output = root / 'grammars' / 'ink.wasm'
    temporary = output.with_suffix('.wasm.tmp')
    try:
        subprocess.run([str(clang), '-fPIC', '-shared', '-Os',
                        '-Wl,--export=tree_sitter_ink', '-I', str(source),
                        str(source / 'parser.c'), str(source / 'scanner.c'),
                        '-o', str(temporary)], env=env, check=True)
        if temporary.read_bytes()[:8] != b'\x00asm\x01\x00\x00\x00':
            raise RuntimeError('Compiler did not produce a WebAssembly module')
        temporary.replace(output)
    finally:
        temporary.unlink(missing_ok=True)
    print(f'Built {output} ({output.stat().st_size:,} bytes)')
    build_native.build(env)
    env['RUSTC_WRAPPER'] = ''
    subprocess.run(['cargo', 'build', '--locked', '--target', 'wasm32-wasip2'], cwd=root, env=env, check=True)
    shutil.copy2(root / 'target/wasm32-wasip2/debug/zed_ink.wasm', root / 'extension.wasm')
    print('Built navigation extension. Retry Install Dev Extension in Zed and select ink-ext.')


if __name__ == '__main__':
    main()
