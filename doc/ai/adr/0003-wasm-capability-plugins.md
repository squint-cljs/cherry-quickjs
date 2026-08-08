# 0003: wasm capability plugins (demo)

Status: demo on the wasm-plugins branch, 2026-08-08

## Context

A bare QuickJS context has no ambient authority: cherry code can only
call what the host injects. That makes cherry-quickjs a candidate for
running agent-written code, if capabilities can be granted explicitly.
wasm modules are a distributable, language-agnostic, sandboxed format
for such capabilities.

## Decision (demo scope)

`--plugin file.wasm` instantiates the module with wasmi (pure Rust
interpreter, keeps the binary small; wasmtime would add ~20MB) and
exposes its exported functions as members of the `plugin` global.

ABI, demo grade:
- A function (i32, i32) -> i64 in a module that also exports `alloc`
  is string -> string. The host allocates, writes utf8 input, passes
  (ptr, len); the i64 result packs the output as (ptr << 32) | len.
- Every other function maps params and result to JS numbers.

demo-plugin/ is a Rust crate compiled to wasm32-unknown-unknown. It
exports `sha256` (string -> hex, a real capability gap: QuickJS has no
crypto) and `add`. The built module is committed as plugins/demo.wasm.

```
$ cherry-quickjs --plugin plugins/demo.wasm \
    -e '(js/plugin.sha256 "hello")'
"2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
```

Without the flag, `plugin` is not defined: deny by default.

## Not in the demo

- WASI grants per plugin (filesystem, network). wasmi has a companion
  wasi crate; this is where real capabilities come from.
- Resource limits on the QuickJS side (memory limit, interrupt handler)
  and on the wasm side (fuel).
- Plugin allocations leak (`alloc` without free); acceptable for
  short-lived instances only.
- A richer ABI (Extism's, or the component model) instead of the packed
  i64 convention.
- Namespacing multiple plugins; all exports land on one `plugin` object.

## Consequences

- Binary grows from 1.8MB to 2.5MB with wasmi included.
- The capability story holds: cherry code cannot obtain what the host
  did not grant.
