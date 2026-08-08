# cherry-quickjs

Cherry scripting in a small (~2MB) cross-platform binary. The
[cherry](https://github.com/squint-cljs/cherry) compiler runs inside an
embedded [QuickJS](https://github.com/quickjs-ng/quickjs) engine via
[rquickjs](https://github.com/DelSkayn/rquickjs).

## Install

Download from the [latest dev release](https://github.com/squint-cljs/cherry-quickjs/releases/tag/dev):

```sh
# macOS (Apple Silicon)
curl -sL https://github.com/squint-cljs/cherry-quickjs/releases/download/dev/cherry-quickjs-0.1.0-macos-aarch64.tar.gz | tar xz
# Linux (x86_64)
curl -sL https://github.com/squint-cljs/cherry-quickjs/releases/download/dev/cherry-quickjs-0.1.0-linux-amd64.tar.gz | tar xz
# Windows (PowerShell)
# Invoke-WebRequest -Uri https://github.com/squint-cljs/cherry-quickjs/releases/download/dev/cherry-quickjs-0.1.0-windows-amd64.zip -OutFile cherry-quickjs.zip
# Expand-Archive cherry-quickjs.zip -DestinationPath .

sudo mv cherry-quickjs /usr/local/bin/  # macOS/Linux
```

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
