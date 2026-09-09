# Ink for Zed

Ink highlighting, comment toggling, bracket matching, an outline, definition/reference
navigation, and autocomplete. The language server and Zed adapter are written in
Rust. The native server links directly to our generated Tree-sitter C parser;
**Node.js and npm are not required at runtime or to build the checked-in sources**.

## Install or rebuild locally

Requirements: Python 3, Rust through rustup (including the `wasm32-wasip2` target),
a native C compiler, and the WASI SDK downloaded by Zed.

From `ink-ext`:

```sh
python3 scripts/build_dev.py
cargo test --locked -p ink-lsp
```

In Zed, run **zed: install dev extension** and select this `ink-ext` directory.
After rebuilding an installed extension, rebuild/reinstall it and run
**editor: restart language server** if the open Ink buffer still uses the old server.

The build generates `extension.toml`, snapshots the local grammar, compiles
`grammars/ink.wasm`, builds a native `ink-lsp`, and embeds that executable in the
Rust extension. The manifest and build artifacts are ignored by Git; the portable
manifest source is `extension.toml.in`. The server is extracted to Zed's extension
work directory and started without Node or a separate installation step.

The development build bundles a server for the machine where it was built.
Rebuild on each target platform. For remote or separately installed servers,
Zed's `lsp.ink-navigation.binary.path` setting can point to a compatible `ink-lsp`.
The adapter reports an explicit error if its bundled platform does not match.

On macOS the build uses Zed's cached WASI SDK. Elsewhere set `WASI_SDK_PATH` or
pass `--wasi-sdk /path/to/wasi-sdk` to the build script. The compiler subprocess
clears host SDK header variables such as `CPATH` to avoid mixing macOS headers
with WebAssembly headers; your shell and Zed settings are unchanged.

## Autocomplete

- After `->`, `<-`, or `->->`, suggest knots, reachable stitches/labels, known
  divert variables, and `END`/`DONE`.
- Filter partly typed targets, including qualified paths such as `hall.room`.
- In expressions and logic, suggest visible variables, parameters, constants,
  list items, functions, and flow names used as visit counts.
- Respect local scopes and `INCLUDE` connections. Ordinary narrative, comments,
  strings, and tags do not get symbol suggestions.

Type `->` to trigger the menu, or invoke Zed's completion action manually.
Completions insert the symbol name; arguments are entered separately.

## Navigation

Place the cursor on a symbol and use **Go to Definition** or **Find All References**.
Supported symbols include knots, stitches, functions, choice/gather labels,
variables, constants, list items, parameters, and temporary variables. Definition
navigation on an `INCLUDE` path opens that file.

References are collected within files connected by `INCLUDE`. Independent stories
in the same workspace remain separate even when they reuse names. Includes are
resolved relative to their source file. Unsaved buffers and UTF-16 positions
(including Chinese and emoji) are supported.

The server does not execute stories, provide compiler diagnostics or rename, or
infer runtime destinations of divert variables. Definition navigation on such a
variable opens its declaration.

## Development and tests

- `lsp/`: native Rust language server, completion, syntax index, and integration tests.
- `src/lib.rs`: Rust/WASM Zed adapter that starts the native server.
- `tree-sitter-ink/`: grammar, C scanner, generated parser, and corpus fixtures.
- `languages/ink/`: Zed configuration and queries.

Run the native server tests (including an actual stdio subprocess):

```sh
cargo test --locked -p ink-lsp
```

The additional grammar/query/recovery/fuzz checks require Tree-sitter CLI 0.26.6:

```sh
python3 scripts/test.py
```

After changing `grammar.js` or the scanner, regenerate with Tree-sitter CLI:

```sh
cd tree-sitter-ink
tree-sitter generate --abi 14
cd ..
python3 scripts/build_dev.py
```

Grammar generation uses JavaScript (Node.js by default, or the CLI's native JS
runtime). This is a build-time parser-generator requirement, not a language-server
dependency. Optional `npm run` shortcuts remain for the same build/test commands;
there are no npm package dependencies.

Tested on Apple Silicon with Tree-sitter 0.26.6 and Zed 1.18.1's WASI SDK.
Automated navigation/completion checks pass; testing the new UI behavior requires
reloading the updated development extension in Zed.

## Image tag previews

Hover over the path in a literal Ink image tag to preview a local image:

```ink
# image: ../images/elevator.svg
# image: "../images/电梯 lobby.png"
```

Relative paths resolve from the `.ink` file's directory. Absolute paths and
`file://` URLs also work. Tags can appear on their own line or after narrative
text. Hovering the path works with unsaved edits; changing the image on disk is
reflected on the next hover.

Supported formats: PNG, JPEG (`.jpg`/`.jpeg`), GIF, WebP, BMP, ICO, TIFF, and SVG.
Previews preserve transparency and fit within 320 × 200 pixels. Raster images
are not enlarged; animated images show a static first frame. SVGs are rendered
with Rust/resvg, using embedded assets and system fonts; external image resources
are not loaded. Web URLs and dynamic Ink expressions are not supported.
Files over 20 MiB, decoding beyond the 256 MiB raster allocation limit, and
missing or invalid images produce an explanatory hover.

After updating, run `python3 scripts/build_dev.py`, reinstall this directory with
Zed's **Install Dev Extension**, then restart the Ink language server. Replace
the example path with an existing image and hover over its filename.
