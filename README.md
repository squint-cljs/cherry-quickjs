# cherry-quickjs

A 1.7MB cross-platform [cherry](https://github.com/squint-cljs/cherry) scripting
binary: the JS build of the cherry compiler runs inside an embedded
[QuickJS](https://github.com/quickjs-ng/quickjs) engine via
[rquickjs](https://github.com/DelSkayn/rquickjs).

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

The cherry compiler and standard library from `assets/` are embedded in
the binary as QuickJS bytecode. Each REPL input is compiled inside the
engine and evaluated in the same context. Startup is around 17ms.

Refresh the assets from a cherry checkout (`bb build` there first) and
record the commit in assets/PROVENANCE:

```bash
cp ../cherry/cljs.core.js assets/
cp ../cherry/lib/{cljs.core,clojure.string,clojure.set,clojure.walk,cljs.pprint,clojure.test,compiler}.js assets/lib/
```
