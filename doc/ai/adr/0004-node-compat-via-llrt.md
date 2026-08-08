# 0004: Node API modules from LLRT

Status: demo on the node-compat branch, 2026-08-08

## Context

Scripts want files, paths and buffers. WinterCG has no filesystem API.
AWS LLRT implements a Node API subset in Rust directly against rquickjs
and publishes the modules as reusable crates.

## Decision (demo scope)

Register llrt_fs, llrt_path and llrt_buffer as native modules under
their node names, next to the embedded cherry modules:

```
user=> (require '["node:fs" :as fs])
nil
user=> (fs/readFileSync "README.md" "utf8")
"# cherry-quickjs\n..."
user=> (require '["node:path" :as path])
nil
user=> (path/join "a" "b" "c.txt")
"a/b/c.txt"
```

`fs`, `path` and `buffer` resolve too. llrt_buffer::init installs the
`Buffer` global. readdirSync, writeFileSync and statSync work; Stats
and Dirent classes come with the module.

## Async

The shell runs on rquickjs's AsyncRuntime over a current-thread tokio
runtime. Promises are awaited as Rust futures (`Promise::into_future`),
so host-resolved promises work: fs/promises and timers are registered,
`llrt_timers::init` installs setTimeout and friends.

```
user=> (require '["node:fs/promises" :as fs])
nil
user=> (count (await (fs/readdir ".")))
17
user=> (await (js/Promise. (fn [resolve _] (js/setTimeout #(resolve :timer-fired) 50))))
:timer-fired
```

After the last eval the shell awaits `AsyncRuntime::idle`, so a pending
timer holds the process open like node:
`-e '(js/setTimeout #(prn :late) 200)'` prints `:late` before exit.

Known limitation: the REPL reads stdin with a blocking read on the
executor thread, so timers only fire while an eval runs or at exit,
not while the REPL waits for input. Async stdin would fix this.

## Consequences

- Binary: 1.8MB -> 2.2MB with fs, fs/promises, path, buffer, timers
  and tokio. Startup ~20ms.
- The llrt 0.8.1-beta crates pin rquickjs 0.11, so this branch
  downgrades from 0.12 (the loader traits lose the ImportAttributes
  parameter, nothing else changed). Upstream tracks rquickjs, so the
  versions should reconverge.
- No capability gating yet: fs is full ambient authority, unlike the
  wasm-plugins branch where --allow scopes filesystem access. The two
  approaches compose: llrt modules for trusted built-ins, wasm plugins
  for untrusted third-party capabilities. Gating llrt fs behind a flag
  (or a path filter) is future work.
- fetch is the natural next module (llrt provides it), behind an
  --allow-net style flag.
