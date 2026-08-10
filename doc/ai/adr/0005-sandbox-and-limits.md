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

Process spawning voids any sandbox: a child process carries the user's
full privileges and none of the runtime's checks (deno documents
`--allow-run` accordingly, and grants like write access to shell rc
files or env vars like `LD_PRELOAD` leak the same way). choq has no
`child_process` today, which is the strongest position. If it lands,
`--sandbox` excludes it entirely rather than offering an
`--allow-run`.

## Stack limits

The quickjs stack limit is 12MB globally (build and runtime), on 16MB
native threads. The driver is the cherry compiler, which recurses per
nested form: the 256KB default dies on a one-line nested form, and
4MB died compiling grenadine's xml.cljc (12KB of nested cond/loop)
under msvc frame sizes on windows. The limit is uniform across
platforms so scripts fail the same everywhere. User code gets the
same 12MB since compiler and user share one engine.

Consequence for `--sandbox`: a tight `--max-stack` for untrusted code
cannot coexist with the compiler's appetite in one engine. The
designed answer is a separate compiler engine (strings in, compiled
js out), created lazily under `--sandbox` only, so normal startup
keeps paying for one engine. The root fix is de-recursing the
compiler hot paths upstream in cherry/squint, which would shrink
stack needs for every embedding.

## Status notes

Recorded ahead of implementation. Suggested order: limits first (pure
engine API), then `--allow-net`, then fs.
