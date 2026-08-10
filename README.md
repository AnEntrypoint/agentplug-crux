# agentplug-crux

Signal-concentration engine for heterogeneous workflow trace logs. Points at
a directory of `.jsonl` transcripts, dedups exact-repeat noise into shapes,
builds frequency/transition/timing baselines over the deduped set, and emits
a small, uninterpreted dump of the statistically rarest shapes. No notion of
"error" or "bad": a value is interesting because it's rare relative to the
corpus, not because of what it means.

Built for Claude Code's own session transcripts at `~/.claude/projects`, but
works on any jsonl-shaped trace corpus. One crate, two faces: a standalone
native CLI (`crux`) and a `wasm32-wasip1` plugin for gm's agentplug host
(`agentplug_crux.wasm`), same pipeline underneath, same output schema.

## CLI usage

```
crux <path-to-jsonl-dir-or-file> [--out dump.jsonl] [--manifest crux-manifest.json]
```

Zero-flag run against a directory writes the dump to stdout and a manifest
next to it. Common overrides:

```
crux ~/.claude/projects \
  --select-percentile 99 --select-min 10 --select-max 300 \
  --weight-field 1.0 --weight-transition 1.5 --weight-timing 1.0 --weight-count 0.5 \
  --smoothing 1.0 \
  --out surprises.jsonl
```

The dump is self-describing: line 1 is a `__meta` schema entry naming and
explaining every field, so a fresh consumer with zero prior context needs
no external docs. Each following line is one deduped "shape": rank, score,
which score component dominated (`dominant_signal`), a representative
canonical event, occurrence count, first/last timestamps (epoch ms and
RFC3339), no narrative, no interpretation. Feed the dump to a subagent
instead of the raw transcripts. The manifest additionally reports the
corpus's top actions/actors by occurrence and the selected score range, for
quick orientation before reading the dump itself.

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
the whole `~/.claude/projects` corpus across every project stays a CLI job.

Dispatch its `scan` verb with `{"root": "."}` (or omit for the project
root) to get `{"ok": true, "dump": [...], "manifest": {...}}` back inline
-- no files written, same schema as the CLI's JSONL dump. `capabilities`
is a no-side-effect probe.

## Pipeline

Ingest (recurse a directory for `.jsonl` files, skipping the same
directories/files gm's own code-index skips -- node_modules, target,
.git, build caches, binaries, etc, see `src/skiplist.rs` -- and honoring
`.gitignore`), Normalize (Claude Code transcript JSON to canonical event:
actor=session id, action=tool name/message role, status=tool result
ok/error, duration_ms=paired tool_use/tool_result timestamps), Dedup
(exact shape hash, quantized durations), Baseline (field-value frequency,
action transition frequency, timing quantiles, all Laplace-smoothed),
Score (weighted surprisal sum), Select (percentile-based, with min/max
floor/ceiling), Emit (JSONL + manifest, or an inline JSON response for the
plugin path).

## Source layout

`event.rs`/`normalize.rs`/`dedup.rs`/`baseline.rs`/`score.rs`/
`skiplist.rs`/`durations.rs`/`emit.rs` are shared, pure logic with no I/O,
compiled into both targets. `native_ingest.rs` (native `std::fs` +
`ignore::WalkBuilder`) and `main.rs` (the CLI) compile for the native
target only; `abi.rs`/`wasm_ingest.rs`/`pipeline.rs` (host-import-driven,
the plugin's `scan` verb) compile for `wasm32-wasip1` only.
