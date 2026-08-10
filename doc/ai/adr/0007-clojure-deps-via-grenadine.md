# 0007: Clojure dependencies via grenadine

Status: accepted, 2026-08-10

## Context

The url import story covers npm. Clojure-ecosystem cljc libraries
(medley and friends) had no way in. tools.deps needs a JVM.
[Grenadine](https://github.com/clojurestar/grenadine) is a portable
tools.deps-shaped resolver in pure Clojure with host effects injected,
published as a vendorable source jar (cc.clojure/grenadine).

## Decision

Vendor grenadine 0.1.6 under `vendor/` (with a char-code patch until
clojurestar/grenadine#5 lands) plus two namespaces of our own:
`choq.deps` (the host map: fs, crypto, zlib, a sync http host fn, a
cljs zip extractor) and a `:cherry` branch in the `clojurestar.deps`
facade.

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
- `mvn:` url-style specifiers
  (`(require '["mvn:group/artifact@1.0.0/some.ns" :as x])`) would
  mirror deno's `npm:`. Blocked on ES module static exports: the shim
  cannot export names it learns only after resolution. The add-deps
  flow does not have this problem because symbol requires bind through
  globalThis.
- Git deps: grenadine supports them, the choq host map does not wire
  them yet.
