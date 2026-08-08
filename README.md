# cherry-quickjs

Cherry scripting in a small (~2MB) cross-platform binary. The
[cherry](https://github.com/squint-cljs/cherry) compiler runs inside an
embedded [QuickJS](https://github.com/quickjs-ng/quickjs) engine via
[rquickjs](https://github.com/DelSkayn/rquickjs).

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
user=> (require '[clojure.string :as s])
nil
user=> (s/join "-" (map inc [1 2 3]))
"2-3-4"
```

`-e` evaluates one expression and prints the non-nil result:

```sh
$ cherry-quickjs -e '(map inc [1 2 3])'
(2 3 4)
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
inside the engine and evaluates it in the same context. Startup takes
about 17ms.
