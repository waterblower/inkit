#!/usr/bin/env python3
"""Grammar, recovery, query, and incremental-edit checks using the CLI and stdlib."""
import os
from pathlib import Path
import re
import subprocess
import tempfile
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
GRAMMAR = ROOT / 'tree-sitter-ink'
CACHE = ROOT / '.cache'
CACHE.mkdir(exist_ok=True)
ENV = {**os.environ, 'XDG_CACHE_HOME': str(CACHE)}


def run(*args, success=True):
    result = subprocess.run(args, cwd=GRAMMAR, env=ENV, capture_output=True, text=True)
    if success and result.returncode:
        raise AssertionError(f'{args}\n{result.stdout}\n{result.stderr}')
    return result


def parse(path, valid=True):
    result = run('tree-sitter', 'parse', '--xml', str(path), success=valid)
    if not valid:
        assert result.returncode == 1 or '<missing_brace ' in result.stdout, result.stdout
    xml = result.stdout[:result.stdout.index('</sources>') + len('</sources>')]
    return ET.fromstring(xml).find('./source/source_file')


def main():
    print(run('tree-sitter', 'test').stdout)
    for fixture in sorted((GRAMMAR / 'test/fixtures').glob('*.ink')):
        parse(fixture)
    with tempfile.TemporaryDirectory(dir=CACHE) as temp:
        path = Path(temp) / 'test.ink'
        # Validate semantic boundaries, not just snapshots of current output.
        cases = [
            ('=== 起点 ===\n= 房间\n', ['knot', 'stitch']),
            ('VARIABLE is prose.\n', ['content']),
            ('VAR score = -2\n', ['variable_declaration']),
            ('{a || b}\n', ['content']),
            ('* [Pick] -> hall.room(1)\n', ['choice']),
        ]
        for source, expected in cases:
            path.write_text(source)
            tree = parse(path)
            assert [node.tag for node in tree] == expected
        path.write_text('{a || b}\n')
        tree = parse(path)
        assert tree.find('./content/inline_expression/binary_expression') is not None
        assert tree.find('.//sequence') is None
        path.write_text('* [Pick] -> hall.room(1)\n')
        tree = parse(path)
        assert [x.text for x in tree.findall('.//path/identifier')] == ['hall', 'room']
        assert tree.find('.//arguments/number').text == '1'
        # Newline and EOF variants preserve structure, including Unicode offsets.
        for newline in ['\n', '\r\n']:
            for final in ['', newline]:
                path.write_bytes((f'=== 起点 ==={newline}你好。{final}').encode())
                tree = parse(path)
                assert [node.tag for node in tree] == ['knot', 'content']
        # Unfinished constructs must leave the following declaration navigable.
        for unfinished in ['* [Unfinished', '{count +', '===', 'VAR value =', '{flag:\nText.']:
            path.write_text(unfinished + '\n=== recovered ===\nSafe prose.\n')
            tree = parse(path, valid=False)
            names = [x.text for x in tree.findall('./knot/identifier')]
            assert 'recovered' in names, (unfinished, ET.tostring(tree))
        # All query files must compile. Also assert outline names and prose safety.
        fixture = GRAMMAR / 'test/fixtures/features.ink'
        for query in sorted((ROOT / 'languages/ink').glob('*.scm')):
            result = run('tree-sitter', 'query', str(query), str(fixture))
            if query.name == 'outline.scm':
                names = re.findall(r' - name,.*text: `([^`]+)`', result.stdout)
                assert names == ['start', 'hall', 'inside', 'double'], names
        path.write_text('Ordinary (parentheses), "quotes", and Chinese “引号”.\n')
        for query in ['brackets.scm', 'highlights.scm']:
            result = run('tree-sitter', 'query', str(ROOT / 'languages/ink' / query), str(path))
            assert 'capture:' not in result.stdout, result.stdout
        path.write_text('VAR x = (2 + 3)\n* [Pick]\n')
        result = run('tree-sitter', 'query', str(ROOT / 'languages/ink/brackets.scm'), str(path))
        assert len(re.findall(r' - open,', result.stdout)) == 2, result.stdout
        assert len(re.findall(r' - close,', result.stdout)) == 2, result.stdout
    print('PASS: syntax fixtures, semantic boundaries, CRLF/EOF, recovery, and Zed queries')
    # Tree-sitter compares fresh and incremental parses across randomized edits.
    result = run('tree-sitter', 'fuzz', '--iterations', '25', '--edits', '3')
    print(result.stdout)


if __name__ == '__main__':
    main()
