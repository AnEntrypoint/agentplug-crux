# crux

Signal-concentration engine for heterogeneous workflow trace logs. Points at
a directory of `.jsonl` transcripts, dedups exact-repeat noise into shapes,
builds frequency/transition/timing baselines over the deduped set, and emits
a small, uninterpreted JSONL dump of the statistically rarest shapes. No
notion of "error" or "bad": a value is interesting because it's rare
relative to the corpus, not because of what it means.

Built for Claude Code's own session transcripts at `~/.claude/projects`, but
works on any jsonl-shaped trace corpus.

## Usage

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
floor/ceiling), Emit (JSONL + manifest).

## As an agentplug plugin

`agentplug-crux` (sibling repo at `../gm/agentplug-crux`) ports this same
pipeline into a `wasm32-wasip1` plugin for gm's agentplug host, scoped to
a project's own directory tree via the host's sandboxed filesystem imports
(`host_fs_read`/`host_fs_readdir`/`host_fs_stat`) instead of native `std::fs`.
Dispatch its `scan` verb with `{"root": "."}` (or omit for the project
root) to get `{"ok": true, "dump": [...], "manifest": {...}}` back inline
-- no files written, same schema as the CLI's JSONL dump.
