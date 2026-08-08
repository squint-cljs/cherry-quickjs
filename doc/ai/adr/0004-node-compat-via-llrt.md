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

## Consequences

- Binary: 1.8MB -> 2.1MB. Startup stays ~16ms.
- The llrt 0.8.1-beta crates pin rquickjs 0.11, so this branch
  downgrades from 0.12 (the loader traits lose the ImportAttributes
  parameter, nothing else changed). Upstream tracks rquickjs, so the
  versions should reconverge.
- Only the sync fs API works: fs/promises functions are async Rust and
  need rquickjs's AsyncRuntime plus tokio to resolve. Registering them
  without an executor gives promises that never settle, so the module
  is not registered. Moving the shell to AsyncRuntime is the path to
  fs/promises, fetch and timers.
- No capability gating yet: fs is full ambient authority, unlike the
  wasm-plugins branch where --allow scopes filesystem access. The two
  approaches compose: llrt modules for trusted built-ins, wasm plugins
  for untrusted third-party capabilities. Gating llrt fs behind a flag
  (or a path filter) is future work.
