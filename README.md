# cherry-quickjs

A 1.7MB cross-platform [cherry](https://github.com/squint-cljs/cherry) scripting
binary: the JS build of the cherry compiler runs inside an embedded
[QuickJS](https://github.com/quickjs-ng/quickjs) engine via
[rquickjs](https://github.com/DelSkayn/rquickjs).

> [!WARNING]
> This prototype was largely written by an LLM. Review before relying on it.

## Build

```bash
cargo build --release
```

## Run

```bash
$ ./target/release/cherry-quickjs
Cherry QuickJS REPL, Ctrl-D to exit
user=> (defn foo [x] (inc x))
#object[foo]
user=> (foo 41)
42
user=> (require '[clojure.string :as s])
nil
user=> (s/join "-" (map inc [1 2 3]))
"2-3-4"
```

`-e` evaluates one expression and prints the non-nil result:

```bash
$ ./target/release/cherry-quickjs -e '(map inc [1 2 3])'
(2 3 4)
```

## How it works

`assets/` holds the ES modules of the `cherry-cljs` npm package: the
compiler, `cljs.core` and the bundled libraries (`clojure.string`,
`clojure.set`, `clojure.walk`, `cljs.pprint`, `clojure.test`). They are
precompiled to QuickJS bytecode by build.rs, embedded into the binary and
served by a custom module resolver/loader, so `import 'cherry-cljs/...'`
works without a filesystem. Each REPL input is compiled inside the engine with
`compileStringEx` in `:repl` mode and evaluated in an async IIFE. Vars
live on `globalThis.<ns>` and persist across evals.

Startup is around 17ms, since no JS is parsed at run time. QuickJS
interprets, so hot loops are slower than V8 or a JITted GraalJS; for
scripting workloads this rarely matters.

The assets must come from a cherry build that includes the repl fixes from
August 2026 (rename collisions, require alias registration). Refresh them
from a cherry checkout with:

```bash
cp ../cherry/cljs.core.js assets/
cp ../cherry/lib/{cljs.core,clojure.string,clojure.set,clojure.walk,cljs.pprint,clojure.test,compiler}.js assets/lib/
```
