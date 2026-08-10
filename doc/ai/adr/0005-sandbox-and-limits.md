# 0005: Sandbox flags and execution limits

Status: proposed, 2026-08-10 (not implemented)

## Context

Choq controls the host layer: every capability the engine has goes
through modules and globals registered from Rust (fs, net, zlib, fetch,
`__listen`, url downloads, process). QuickJS itself has no ambient
authority. That makes a deno-style permission layer enforceable at a
handful of choke points, which is not true for runtimes where scripts
reach the OS directly.

A sandboxed choq targets running untrusted or LLM-generated code:
fresh engine per task costs ~18ms, and the interpreter (no JIT) keeps
the attack surface small.

Findings that shape the design:

- llrt_net ships `security.rs` with `set_allow_list`/`set_deny_list`,
  enforced per address inside connect and listen. `--allow-net` is
  mostly plumbing CLI flags into existing enforcement. llrt_fs likely
  has the same mechanism (verify).
- QuickJS supports `set_interrupt_handler` (polled every ~10k VM
  instructions), `set_memory_limit` and `set_max_stack_size`. Measured
  cost of an always-on interrupt handler: ~1.7% on a pure interpreter
  loop, noise on startup and on io-bound work. Timeouts can kill
  synchronous infinite loops, which node and bun cannot.

## Decision (proposed)

Flags:

- `--sandbox`: deny-all baseline. No fs, no net, no env, no serve.
- `--allow-read[=paths]`, `--allow-write[=paths]`: fs grants. v1 may
  be module-level (register fs or not); path granularity via llrt fs
  lists if present.
- `--allow-net[=hosts]`: llrt_net lists plus a host check in
  `fetch_url` and gates on `fetch` and `__listen`.
- `--allow-env`: expose `process.env`, otherwise filter it out.
- `--allow-import`: url imports at require time, gated separately from
  `--allow-net` like deno does; the future choq.lock guards integrity.
- `--timeout <dur>` and `--max-heap <size>`: engine limits, usable
  with or without `--sandbox`.

Denials throw at the point of use with a message naming the missing
flag, deno-style.

Examples:

```sh
choq --sandbox --timeout 5s --max-heap 64mb untrusted.cljs
choq --sandbox --allow-read=./data --allow-net=api.openai.com task.cljs
choq --sandbox --timeout 2s -e "$GENERATED_CODE"
```

## Caveats

In-process sandboxing is bounded by QuickJS memory safety. For hostile
code, wrap OS-level isolation around the process as well. This layer
is defense in depth, not a security boundary on its own.

## Status notes

Recorded ahead of implementation. Suggested order: limits first (pure
engine API), then `--allow-net`, then fs.
