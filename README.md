# cherry-quickjs

Cherry scripting in a small (~4MB) cross-platform binary. The
[cherry](https://github.com/squint-cljs/cherry) compiler runs inside an
embedded [QuickJS](https://github.com/quickjs-ng/quickjs) engine via
[rquickjs](https://github.com/DelSkayn/rquickjs).

## Status

Experimental.

## Install

macOS and Linux:

```sh
curl -sL https://raw.githubusercontent.com/squint-cljs/cherry-quickjs/main/install | bash
# or into a specific directory:
curl -sL https://raw.githubusercontent.com/squint-cljs/cherry-quickjs/main/install | bash -s -- --dir ~/bin
```

On Windows, download the zip from the
[dev release](https://github.com/squint-cljs/cherry-quickjs/releases/tag/dev).

## Usage

```sh
$ cherry-quickjs
Cherry QuickJS REPL, Ctrl-D to exit
user=> (defn foo [x] (inc x))
#object[foo]
user=> (foo 41)
42
user=> (require '[clojure.string :as str])
nil
user=> (str/join "-" (map inc [1 2 3]))
"2-3-4"
```

`-e` evaluates one expression and prints the non-nil result:

```sh
$ cherry-quickjs -e '(map inc [1 2 3])'
(2 3 4)
```

A file argument runs the file:

```sh
$ cherry-quickjs script.cljs
```

`--nrepl` starts an nREPL server and writes `.nrepl-port` for editor
connections:

```sh
$ cherry-quickjs --nrepl        # port 1339
$ cherry-quickjs --nrepl 7888
```

## API

Cherry namespaces, embedded:
`clojure.string`, `clojure.set`, `clojure.walk`, `cljs.pprint`,
`clojure.test`.

Node modules, from [LLRT](https://github.com/awslabs/llrt), also under
their `node:` names: `fs`, `fs/promises`, `path`, `buffer`, `timers`,
`tty`, `crypto`, `net`, `os`, `process`.
[API.md](https://github.com/awslabs/llrt/blob/main/API.md) lists the
functions that each module supports. `stream`, `events` and
`string_decoder` are vendored js implementations
([readable-stream](https://github.com/nodejs/readable-stream)).

Globals: `Buffer`, `console`, `crypto`, `process`, `fetch`, `Headers`,
`Request`, `Response`, `FormData`, `setTimeout`, `clearTimeout`,
`setInterval`, `clearInterval`, `setImmediate`.

`*e` holds the last repl exception: `(.-stack *e)` prints where it
happened.

`await` works on all promises:

```clojure
(require '["node:fs/promises" :as fs])
(await (fs/readFile "README.md" "utf8"))

(-> (await (js/fetch "https://api.github.com/zen")) (.text) await)
```

## HTTP server

`cherry.http/serve` starts an HTTP server on 127.0.0.1. The handler
takes a fetch API `Request` and returns a `Response`, or a promise of
one:

```clojure
(require '[cherry.http :refer [serve]])

(serve (fn [req] (js/Response. "hello")) {:port 3000})
```

A Hono app plugs in directly:

```clojure
(require '["https://esm.sh/hono" :refer [Hono]]
         '[cherry.http :refer [serve]])

(def app (Hono.))
(.get app "/" (fn [c] (.text c "hello from hono")))

(serve (.-fetch app) {:port 3000})
```

A script with a running server keeps the process alive. JS modules
import the same API from `cherry:http`.

## URL imports

`https://` specifiers load like in Deno. Downloads cache in
`~/.cache/cherry-quickjs`:

```clojure
(require '["https://esm.sh/lodash-es@4.17.21" :as l])
(l/camelCase "foo bar")
```

Pin the version in the URL. An unpinned URL stays at the version that
the first download returned. There is no integrity check.

Downloads send a Node user agent, so esm.sh serves node builds instead
of browser builds. Node builtin imports in remote modules (`node:fs`
and friends, or esm.sh's `/node/*.mjs` shims) resolve to the native
modules above, so npm libraries that use them work:

```clojure
(require '["https://esm.sh/@babashka/fs" :as bfs])
(bfs/slurp "README.md")
```

## Build from source

```sh
pnpm install
cargo build --release
```

Run the tests:

```sh
target/debug/cherry-quickjs test/nrepl_test.cljs
```

## Implementation

The compiler and the standard library come from the
[cherry-cljs](https://www.npmjs.com/package/cherry-cljs) npm package,
pinned in package.json. build.rs compiles these modules to QuickJS
bytecode and embeds them in the binary. The REPL compiles each input
inside the engine and evaluates it in the same context. The node
modules are Rust, from LLRT, on a tokio event loop. Startup takes
about 20ms.
