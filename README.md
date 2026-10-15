# agentplug-crux

Signal-concentration engine for large, low-signal-density corpora. Points
at a directory, dedups exact-repeat noise into shapes, builds
frequency/transition/timing baselines over the deduped set, and emits a
small, uninterpreted dump of the statistically rarest shapes. No notion of
"error" or "bad": a value is interesting because it's rare relative to the
corpus, not because of what it means. The scoring, dedup, selection,
context-window, and cross-reference logic has no format-specific
assumptions -- it operates on the generic `CanonicalEvent` shape
(actor/action/status/duration/fields), so the same pipeline works over
different kinds of corpora via different ingest **modes** (`--mode`):

- **`jsonl`** (default): trace logs -- one event per jsonl record. Claude
  Code session transcripts, other agent frameworks' logs, workflow-engine
  traces, service-mesh logs. `normalize.rs` is the one format-specific
  stage here; a different jsonl record shape needs its own mapping there.
- **`files`**: any codebase or file tree -- one event per file (extension,
  size, depth, directory), no content parsing. Surfaces structural
  outliers: a file with a rare extension, a file far outside its
  extension's typical size band for this corpus, an oddly-placed lone
  config file. Not a code-search or AST tool -- it never reads a file's
  content, so it finds a different kind of signal than those tools do.
- **`gitlog`**: a git repository's history -- one event per
  (commit, changed file) via `git log --numstat`. Surfaces history
  outliers: unusually large changes, rare author/file-type combinations.
  Native CLI only (shells out to `git`; not available to the sandboxed
  wasm plugin).

One crate, two faces: a standalone native CLI (`crux`, all three modes)
and a `wasm32-wasip1` plugin for gm's agentplug host
(`agentplug_crux.wasm`, jsonl mode only, see below), same pipeline
underneath, same output schema.

## CLI usage

```
crux [--mode jsonl|files|gitlog] <path...> [--out dump.jsonl] [--manifest crux-manifest.json]
```

Zero-flag run against a directory (jsonl mode) writes the dump to stdout
and a manifest next to it. Common overrides:

```
crux ~/.claude/projects \
  --select-percentile 99 --select-min 10 --select-max 300 \
  --weight-field 1.0 --weight-transition 1.5 --weight-timing 1.0 --weight-count 0.5 \
  --smoothing 1.0 --context-window 3 \
  --out surprises.jsonl

crux --mode files ~/some/codebase --out structure-surprises.jsonl
crux --mode gitlog ~/some/repo --max-commits 5000 --out history-surprises.jsonl
```

The dump is self-describing: line 1 is a `__meta` schema entry naming and
explaining every field, so a fresh consumer with zero prior context needs
no external docs. Each following line is one deduped "shape": rank, score,
which score component dominated (`dominant_signal`), a representative
canonical event, occurrence count, first/last timestamps (epoch ms and
RFC3339), up to `--context-window` raw events immediately before/after it
in the same source file (`context_before`/`context_after`, verbatim
scaffolding, not re-scored -- for `gitlog` mode this means the
chronologically adjacent changes to that same file), and
`cross_references` -- the `shape_id`s of other selected shapes sharing the
same actor (session id for jsonl mode, commit author for gitlog mode) --
so the relational signal between two individually unremarkable shapes
survives, not just each shape in isolation. No narrative, no
interpretation. Feed the dump to a subagent instead of the raw corpus.
The manifest additionally reports the corpus's top actions/actors by
occurrence and the selected score range, for quick orientation before
reading the dump itself.

`files`/`gitlog` modes use a minimal noise-only skip list (VCS internals
and confirmed build-output directory names only -- `node_modules`,
`target`, `dist`, `build`, `.venv`, `__pycache__`) rather than jsonl
mode's larger code-index-tuned list, since a generically-named directory
(`site`, `weights`, a local `.cargo/config.toml`) can be real, authored
content worth scoring -- hiding it first would defeat the "let rarity
decide" premise these modes exist for.

### Near-duplicate detection (`--mode files`, `--ncd-threshold`)

Exact-shape dedup collapses byte-identical repeats, but structurally
cannot see near-duplicates: a copy-pasted config with one field changed,
or a vendored variant of the same source file, hash completely
differently despite being 99% the same content. `--ncd-threshold <0..1>`
(disabled by default, opt-in since it's real extra work) adds Normalized
Compression Distance -- `(C(a+b) - min(C(a),C(b))) / max(C(a),C(b))` via
gzip, pure Rust (`flate2`'s `rust_backend`, no C toolchain, compiles
identically on both targets even though this specific feature is CLI-only
today) -- as a content-aware signal on top of the otherwise purely
structural/statistical scoring. For each *selected* shape (never every
file against every other file -- bounded to O(selected × corpus), with a
same-extension + same-order-of-magnitude-size prefilter that skips the
actual gzip calls for pairs that could not plausibly qualify), other
corpus files scoring at or below the threshold appear in that shape's
`near_duplicates` field, sorted most-similar first.

**Known limitation, by design, not a bug:** DEFLATE's sliding window is a
hard-capped 32KB, so NCD stops being meaningful once two files (or their
shared region) exceed a few multiples of that -- confirmed empirically: a
real ~180KB near-duplicate pair (99% identical, a handful of changed
lines in an otherwise-identical file) scored NCD≈0.98 (maximally
"different") before a size cap existed, a false negative caused purely by
file size. Files above 96KB are never compared (no `near_duplicates`
entry for them, not an error) -- this feature reliably covers
small-to-medium files; large-file near-duplication is a known gap.

## As an agentplug plugin

`cargo build --release --target wasm32-wasip1 --lib` builds
`agentplug_crux.wasm`, matching agentplug's plugin ABI
(`plugin_call`/`plugkit_alloc`/`plugkit_free`, see `../agentplug/docs/ABI.md`)
via `src/abi.rs`, structured identically to the sibling `agentplug-bert`/
`agentplug-libsql`/`agentplug-treesitter` plugins. Scoped to a project's own
directory tree via the host's sandboxed filesystem imports
(`host_fs_read`/`host_fs_readdir`/`host_fs_stat`) instead of native
`std::fs` -- the plugin cannot reach outside the calling project's own
`cwd` (plus `~/.gm`), so `scan` complements the CLI rather than replacing
it: a project auditing its own `.jsonl` traces uses the plugin, auditing
a whole external corpus, a codebase's structure, or git history stays a
CLI job (the plugin's `scan` verb only implements jsonl-mode ingest today).

Dispatch its `scan` verb with `{"root": "."}` (or omit for the project
root) to get `{"ok": true, "dump": [...], "manifest": {...}}` back inline
-- no files written, same schema as the CLI's JSONL dump, same
`context_window`/weights/select_* knobs in the body. `capabilities` is a
no-side-effect probe.

## Pipeline

Ingest (mode-specific -- see above; the directory walk in every mode
honors `.gitignore`), Normalize (jsonl mode only: raw record to canonical
event; files/gitlog modes build the canonical event directly during
ingest since there's no separate record to parse), Dedup (exact shape
hash, quantized durations/sizes), Baseline (field-value frequency, action
transition frequency, timing quantiles, all Laplace-smoothed), Score
(weighted surprisal sum), Select (percentile-based, with min/max
floor/ceiling), Emit (JSONL + manifest, or an inline JSON response for the
plugin path -- both include context windows and cross-references per
selected shape).

## Source layout

`event.rs`/`normalize.rs`/`dedup.rs`/`baseline.rs`/`score.rs`/
`skiplist.rs`/`durations.rs`/`context.rs`/`emit.rs`/`ncd.rs` are shared,
pure logic with no I/O, compiled into both targets. `walk.rs` (shared
directory-walk helper over `ignore::WalkBuilder`, native-only),
`native_ingest.rs` (jsonl mode), `ingest_files_mode.rs` (files mode),
`ingest_gitlog.rs` (gitlog mode, shells out to `git`), `near_dup.rs`
(files-mode near-duplicate clustering, real file I/O), and `main.rs` (the
CLI, mode selection) compile for the native target only;
`abi.rs`/`wasm_ingest.rs`/`pipeline.rs` (host-import-driven, the plugin's
`scan` verb, jsonl mode only) compile for `wasm32-wasip1` only.
