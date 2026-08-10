use rquickjs::loader::{Loader, Resolver};
use rquickjs::module::Declared;
use rquickjs::{Context, Ctx, Error, Module, Runtime, WriteOptions};
use std::path::Path;
use std::{env, fs};

// modules nothing else imports; their deps are pulled in through the loader
const ROOTS: &[&str] = &[
    "cherry-cljs/cljs.core.js",
    "cherry-cljs/lib/compiler.js",
    "cherry-cljs/lib/clojure.set.js",
    "cherry-cljs/lib/cljs.pprint.js",
    "cherry-cljs/lib/clojure.test.js",
    "cherry-cljs/lib/node.nrepl_server.js",
];

// node builtins the runtime provides; declared empty here so bytecode
// compiles, never written to disk
const BUILTIN_STUBS: &[&str] = &[
    "net",
    "fs",
    "path",
    "crypto",
    "child_process",
    "node:crypto",
    "node:fs",
    "node:os",
    "node:zlib",
];

// vendored cljc, compiled to js by the embedded cherry compiler at
// build time, in dependency order (the compiler threads ns state)
const CLJC: &[(&str, &str)] = &[
    ("grenadine.version", "vendor/grenadine/version.cljc"),
    ("grenadine.xml", "vendor/grenadine/xml.cljc"),
    ("grenadine.expander", "vendor/grenadine/expander.cljc"),
    ("grenadine.gitlibs", "vendor/grenadine/gitlibs.cljc"),
    ("grenadine.source", "vendor/grenadine/source.cljc"),
    ("grenadine.pom", "vendor/grenadine/pom.cljc"),
    ("grenadine.lock", "vendor/grenadine/lock.cljc"),
    ("grenadine.repo", "vendor/grenadine/repo.cljc"),
    ("grenadine.coordinate", "vendor/grenadine/coordinate.cljc"),
    ("grenadine.graph", "vendor/grenadine/graph.cljc"),
    ("grenadine.basis", "vendor/grenadine/basis.cljc"),
    ("grenadine.core", "vendor/grenadine/core.cljc"),
    ("grenadine.runtime", "vendor/grenadine/runtime.cljc"),
    ("choq.deps", "vendor/choq/deps.cljs"),
    ("clojurestar.deps", "vendor/clojurestar/deps.cljc"),
];

fn asset_path(name: &str) -> String {
    format!("node_modules/{}", name)
}

fn out_path(name: &str) -> std::path::PathBuf {
    Path::new(&env::var("OUT_DIR").unwrap()).join(name.replace('/', "_"))
}

fn normalize(path: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    out.join("/")
}

struct AssetResolver;

impl Resolver for AssetResolver {
    fn resolve<'js>(
        &mut self,
        _ctx: &Ctx<'js>,
        base: &str,
        name: &str,
    ) -> rquickjs::Result<String> {
        if name.starts_with("./") || name.starts_with("../") {
            let dir = base.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
            Ok(normalize(&format!("{}/{}", dir, name)))
        } else {
            Ok(name.to_string())
        }
    }
}

struct AssetLoader;

fn declare_and_write<'js>(ctx: &Ctx<'js>, name: &str) -> rquickjs::Result<Module<'js, Declared>> {
    let src = fs::read_to_string(asset_path(name))
        .map_err(|_| Error::new_loading(name))?;
    let module = Module::declare(ctx.clone(), name, src)?;
    let bytes = module.write(WriteOptions {
        strip_source: true,
        ..Default::default()
    })?;
    fs::write(out_path(name), bytes).unwrap();
    Ok(module)
}

impl Loader for AssetLoader {
    fn load<'js>(
        &mut self,
        ctx: &Ctx<'js>,
        name: &str,
    ) -> rquickjs::Result<Module<'js, Declared>> {
        if BUILTIN_STUBS.contains(&name) {
            return Module::declare(ctx.clone(), name, "");
        }
        declare_and_write(ctx, name)
    }
}

fn main() {
    println!("cargo:rerun-if-changed=package.json");
    println!("cargo:rerun-if-changed=pnpm-lock.yaml");
    if !Path::new("node_modules/cherry-cljs").exists() {
        panic!("node_modules/cherry-cljs not found, run `pnpm install` first");
    }
    println!("cargo:rerun-if-changed=vendor");
    let rt = Runtime::new().unwrap();
    rt.set_max_stack_size(4 * 1024 * 1024);
    rt.set_loader(AssetResolver, AssetLoader);
    let context = Context::full(&rt).unwrap();
    context.with(|ctx| {
        for name in ROOTS {
            if let Err(e) = declare_and_write(&ctx, name) {
                panic!("declaring {}: {} {:?}", name, e, ctx.catch());
            }
        }
    });

    // compile the vendored cljc to js with the embedded compiler
    context.with(|ctx| {
        ctx.eval::<(), _>(
            "import('cherry-cljs/lib/compiler.js')\
               .then(m => { globalThis.__c = m; })\
               .catch(e => { globalThis.__cerr = String(e); })",
        )
        .unwrap();
    });
    while rt.execute_pending_job().unwrap_or(false) {}
    context.with(|ctx| {
        let globals = ctx.globals();
        if let Ok(err) = globals.get::<_, String>("__cerr") {
            panic!("loading cherry compiler: {}", err);
        }
        let _: rquickjs::Value = globals.get("__c").expect("cherry compiler not loaded");
        for (ns, path) in CLJC {
            let src = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {}", path, e));
            globals.set("__src", src).unwrap();
            let js: String = ctx
                .eval(
                    "(() => {\
                       const res = __c.compileStringEx(\
                         __src,\
                         {repl: true, context: 'return', elide_exports: true},\
                         globalThis.__st ?? null);\
                       globalThis.__st = res;\
                       return res.javascript;\
                     })()",
                )
                .unwrap_or_else(|e| panic!("compiling {}: {} {:?}", ns, e, ctx.catch()));
            fs::write(out_path(&format!("cljc.{}.js", ns)), js).unwrap();
        }
    });
}
