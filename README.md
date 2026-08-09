# crux

Signal-concentration engine for heterogeneous workflow trace logs. Points at
a directory of `.jsonl` transcripts, dedups exact-repeat noise into shapes,
builds frequency/transition/timing baselines over the deduped set, and emits
a small, uninterpreted JSONL dump of the statistically rarest shapes. No
notion of "error" or "bad" — a value is interesting because it's rare
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
  --weights-field 1.0 --weight-transition 1.5 --weight-timing 1.0 --weight-count 0.5 \
  --smoothing 1.0 \
  --out surprises.jsonl
```

Each output line is one deduped "shape": a representative canonical event,
its occurrence count, first/last timestamps, and a score breakdown
(field/transition/timing/count surprisal) — no narrative, no interpretation.
Feed the dump to a subagent instead of the raw transcripts.

## Pipeline

Ingest (recurse `.jsonl` files) → Normalize (Claude Code transcript JSON →
canonical event: actor=session id, action=tool name/message role,
status=tool result ok/error, duration_ms=paired tool_use→tool_result
timestamps) → Dedup (exact shape hash, quantized durations) → Baseline
(field-value frequency, action transition frequency, timing quantiles, all
Laplace-smoothed) → Score (weighted surprisal sum) → Select
(percentile-based, with min/max floor/ceiling) → Emit (JSONL + manifest).
