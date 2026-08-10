# 0007: Clojure dependencies via grenadine

Status: accepted, 2026-08-10

## Context

The url import story covers npm. Clojure-ecosystem cljc libraries
(medley and friends) had no way in. tools.deps needs a JVM.
[Grenadine](https://github.com/clojurestar/grenadine) is a portable
tools.deps-shaped resolver in pure Clojure with host effects injected,
published as a vendorable source jar (cc.clojure/grenadine).

## Decision

build.rs downloads the pinned grenadine source jar from clojars
(sha256 verified, cached in `~/.cache/choq/build`), extracts it and
applies the patches in `patches/` (a char-code fix until
clojurestar/grenadine#5 lands, and a `:cherry` branch in the
`clojurestar.deps` facade). `src/choq/deps.cljs` adds the host map:
fs, crypto, zlib, a sync http host fn and a cljs zip extractor. Only
`choq.deps` resolves from user code; the grenadine namespaces are
internal.

build.rs compiles the vendored cljc to js with the embedded cherry
compiler (no node in the build) and the loader serves each namespace
as a module wrapped in an async iife. Runtime cost of compiling
grenadine on first use would have been 2.1s, so precompilation is
mandatory. Binary cost: +280KB. Startup: unchanged, modules load on
first require.

`add-deps` registers extracted source roots with the runtime; bare
namespace requires then resolve through them (the same mechanism as
local `src`/`test` namespaces). Jars cache in `~/.m2/repository`.

## Open

- Config file: `cherry.edn`, `choq.edn`, or an alias in `deps.edn`.
  Undecided. Grenadine consumes tools.deps-shaped maps from any of
  them.
- `mvn:` specifiers
  (`(require '["mvn:group/artifact@1.0.0/some.ns" :as x])`) are
  served by the module loader in every eval path: `deps::load_mvn`
  ensures the dependency and compiles the namespace synchronously
  (the loader is a host callback, re-entrant ctx.eval is fine, and
  the grenadine host is sync), then emits a module whose exports are
  the vars the compiled output assigns. The one wrinkle: the sync
  loader cannot pull in choq.deps itself, so it must already be
  loaded. The nrepl boot imports it, and `__evalCherry` lazily
  imports it when the source mentions mvn:, keeping startup unchanged
  for code that does not. Known upgrade path that removes the source
  sniff: make the choq.deps chain synchronously evaluable with a
  build-time transform (concatenate the compiled modules in topo
  order, strip the ordering-only awaited imports, take cljs.core and
  the native modules from namespaces the bootstrap stashes), so the
  loader can eval it just in time. That also makes `import('mvn:...')`
  work from pure js contexts that never pass through the bootstrap.
  Pair it with the choq:internal hygiene pass when that happens.
- Git deps: grenadine supports them, the choq host map does not wire
  them yet.
- The compile cache keys on source content only. Macros break that
  two ways: a namespace using macros from another caches expansions
  that go stale when the macro source changes, and a cache hit skips
  the compile that would register macros for later consumers. Needs
  dependency-aware keys plus persisted ns-state, or a cache bypass
  for macro-defining namespaces. The cherry-cljs version also belongs
  in the key.
