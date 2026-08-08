# 0001: Embed the JS cherry compiler in QuickJS

Status: accepted, 2026-08-07

## Context

Goal: a small cross-platform cherry scripting binary. The GraalVM
native-image prototype (graaljs-cherry) works but is 101MB with JIT, 49MB
without, and compiles cherry on the JVM. Cross-platform requirements rule
out the macOS JavaScriptCore system framework (smallest option, ~100KB
shell). V8 via rusty_v8 builds 30-40MB binaries. QuickJS (quickjs-ng via
the rquickjs Rust bindings) is ~1MB, compiles everywhere with cc, and
supports full ES modules including dynamic import.

## Decision

Self-host the compiler: run the JS build of cherry (`lib/compiler.js`
from the cherry-cljs package) inside the engine instead of compiling
outside it. Rust is a thin shell (~200 lines): stdin loop, module
loader, job-queue driving.

- `assets/` holds the ES modules: compiler, cljs.core and the bundled
  libraries. build.rs runs QuickJS at build time, pulls all modules
  through the same resolver/loader the binary uses, and serializes each
  to bytecode (source stripped, debug info kept for stack traces).
  Loading bytecode skips parsing: startup dropped 76ms -> 17ms. The
  remaining time is module init (cljs.core builds the standard library
  at load). QuickJS has no heap snapshot, so that is the floor.
- A custom Resolver/Loader pair serves the embedded modules by name, so
  `import 'cherry-cljs/...'` and relative imports between the assets work
  without a filesystem. QuickJS resolves static imports eagerly at
  declare time, which is why build.rs needs the loader too.
- Eval strategy mirrors squint's nREPL server and graaljs-cherry:
  `compileStringEx` with `{repl: true, context: 'return', elide_exports:
  true}`, threading the returned state across evals, wrapping in an async
  IIFE, printing via `pr_str`. Vars persist on `globalThis.<ns>`.
  Incomplete input is detected by the "EOF while reading" compile error.
- The JS glue (bootstrap, console shim over a Rust `__print`) lives in
  string constants in main.rs. The console shim exists because bare
  QuickJS contexts have no console and cljs.core's print functions
  expect one.

## Consequences

- 1.8MB binary, ~17ms startup, ~6.5MB peak memory.
- Interpreter-only: hot loops are ~50x slower than JITted GraalJS.
- The assets must come from a cherry build with the August 2026 repl
  fixes. The published cherry-cljs npm package (as of 0.5.x) reproduces
  two bugs in a fresh engine: "'seq__37' is read-only" (let-rename
  collision between the top-level gensym counter and the per-compile
  counter) and "s is not defined" on a second require (re-registration
  of aliases from earlier evals). Both were fixed upstream in squint and
  cherry; refresh assets from a cherry checkout until a release ships.
- No npm package loading and no Node builtins. A node_modules fallback in
  the resolver (port of graaljs-cherry's package.json exports resolution)
  is the natural extension.
- Bytecode is QuickJS-version-specific. build.rs and the binary use the
  same rquickjs version, so this only matters when upgrading rquickjs.
