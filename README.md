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

## API

Cherry namespaces, embedded:
`clojure.string`, `clojure.set`, `clojure.walk`, `cljs.pprint`,
`clojure.test`.

Node modules, from [LLRT](https://github.com/awslabs/llrt), also under
their `node:` names: `fs`, `fs/promises`, `path`, `buffer`, `timers`.
[API.md](https://github.com/awslabs/llrt/blob/main/API.md) lists the
functions that each module supports.

Globals: `Buffer`, `console`, `fetch`, `Headers`, `Request`,
`Response`, `FormData`, `setTimeout`, `clearTimeout`, `setInterval`,
`clearInterval`, `setImmediate`.

`await` works on all promises:

```clojure
(require '["node:fs/promises" :as fs])
(await (fs/readFile "README.md" "utf8"))

(-> (await (js/fetch "https://api.github.com/zen")) (.text) await)
```

## Build from source

```sh
pnpm install
cargo build --release
```

## Implementation

The compiler and the standard library come from the
[cherry-cljs](https://www.npmjs.com/package/cherry-cljs) npm package,
pinned in package.json. build.rs compiles these modules to QuickJS
bytecode and embeds them in the binary. The REPL compiles each input
inside the engine and evaluates it in the same context. The node
modules are Rust, from LLRT, on a tokio event loop. Startup takes
about 20ms.
