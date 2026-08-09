# Choq

Choq (Cherry on QuickJS) is a ~5MB binary chock full of scripting
goodies. It runs the [cherry](https://github.com/squint-cljs/cherry)
compiler inside an embedded
[QuickJS](https://github.com/quickjs-ng/quickjs) engine via
[rquickjs](https://github.com/DelSkayn/rquickjs).

## Status

Experimental.

## When to use choq

Choq trades peak performance for size and memory. QuickJS has no JIT,
so hot code runs slower than on Node.js, Bun or Deno, but the binary
is small, startup is fast and memory use stays low: in local
measurements a Hono app serves around 30k requests/s and uses less
memory than the same app on Node.js or Bun.

Use it as a lighter alternative to Node.js for scripts and small
servers, in the same spirit as babashka next to the JVM: personal
projects on a VPS, small boards like a Raspberry Pi, or anywhere a
full JS runtime is too heavy.

## Install

The install script puts `choq` in `/usr/local/bin`. Use `sudo` when
that directory is not writable, or pick another directory with
`--dir`:

```sh
curl -sL https://raw.githubusercontent.com/squint-cljs/choq/main/install | sudo bash
# into a specific directory, no sudo:
curl -sL https://raw.githubusercontent.com/squint-cljs/choq/main/install | bash -s -- --dir ~/bin
```

Works on macOS, Linux and Windows (Git Bash). Binaries are also on the
[dev release](https://github.com/squint-cljs/choq/releases/tag/dev).

## Usage

```sh
$ choq
Choq REPL, Ctrl-D to exit
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
$ choq -e '(map inc [1 2 3])'
(2 3 4)
```

Pass a file argument to run a script:

```sh
$ choq script.cljs
```

`--nrepl` starts an nREPL server and writes `.nrepl-port` for editor
connections:

```sh
$ choq --nrepl        # port 1339
$ choq --nrepl 7888
```

## API

Embedded cherry namespaces: `clojure.string`, `clojure.set`,
`clojure.walk`, `cljs.pprint`, `clojure.test`.

Node modules from [LLRT](https://github.com/awslabs/llrt), also
available under their `node:` names: `fs`, `fs/promises`, `path`,
`buffer`, `timers`, `tty`, `crypto`, `net`, `os`, `process`, `zlib`.
[API.md](https://github.com/awslabs/llrt/blob/main/API.md) lists the
functions that each module supports. `stream`, `events` and
`string_decoder` are vendored js implementations based on
[readable-stream](https://github.com/nodejs/readable-stream).

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

`choq.http/serve` starts an HTTP server on 127.0.0.1. The handler
takes a fetch API `Request` and returns a `Response`, or a promise of
one:

```clojure
(require '[choq.http :refer [serve]])

(serve (fn [req] (js/Response. "hello")) {:port 3000})
```

A Hono app plugs in directly:

```clojure
(require '["https://esm.sh/hono" :refer [Hono]]
         '[choq.http :refer [serve]])

(def app (Hono.))
(.get app "/" (fn [c] (.text c "hello from hono")))

(serve (.-fetch app) {:port 3000})
```

A running server keeps the process alive. JS modules import the same
API from `choq:http`.

## URL imports

`https://` specifiers load like in Deno. Downloads are cached in
`~/.cache/choq`:

```clojure
(require '["https://esm.sh/lodash-es@4.17.21" :as l])
(l/camelCase "foo bar")
```

Pin the version in the URL. An unpinned URL stays at the version that
the first download returned.

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
target/debug/choq test/nrepl_test.cljs
```

## Implementation

The compiler and the standard library come from the
[cherry-cljs](https://www.npmjs.com/package/cherry-cljs) npm package,
pinned in package.json. build.rs compiles these modules to QuickJS
bytecode and embeds them in the binary. The REPL compiles each input
inside the engine and evaluates it in the same context. The node
modules are LLRT's Rust implementations, driven by a tokio event loop.
Startup takes about 20ms.
