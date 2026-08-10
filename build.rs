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

// grenadine arrives as the pinned clojars source jar, verified and
// extracted at build time; patches/ carries our local changes
const GRENADINE_VERSION: &str = "0.1.6";
const GRENADINE_SHA256: &str = "f7dbb9fa5d9998e8c6460d7995088ec98551b5ec8cb6ee568a7fa3e7b1fc1e6b";

const PATCHES: &[(&str, &str)] = &[
    ("patches/grenadine-xml.patch", "grenadine/xml.cljc"),
    ("patches/grenadine-version.patch", "grenadine/version.cljc"),
    ("patches/clojurestar-deps.patch", "clojurestar/deps.cljc"),
];

// cljc compiled to js by the embedded cherry compiler at build time,
// in dependency order (the compiler threads ns state); grenadine paths
// are relative to the extracted jar, choq.deps lives in this repo
const CLJC: &[(&str, &str)] = &[
    ("grenadine.version", "grenadine/version.cljc"),
    ("grenadine.xml", "grenadine/xml.cljc"),
    ("grenadine.expander", "grenadine/expander.cljc"),
    ("grenadine.gitlibs", "grenadine/gitlibs.cljc"),
    ("grenadine.source", "grenadine/source.cljc"),
    ("grenadine.pom", "grenadine/pom.cljc"),
    ("grenadine.lock", "grenadine/lock.cljc"),
    ("grenadine.repo", "grenadine/repo.cljc"),
    ("grenadine.coordinate", "grenadine/coordinate.cljc"),
    ("grenadine.graph", "grenadine/graph.cljc"),
    ("grenadine.basis", "grenadine/basis.cljc"),
    ("grenadine.core", "grenadine/core.cljc"),
    ("grenadine.runtime", "grenadine/runtime.cljc"),
    ("choq.deps", "src/choq/deps.cljs"),
    ("clojurestar.deps", "clojurestar/deps.cljc"),
];

// download (or reuse from the cache), verify, extract, patch; returns
// the directory holding the jar sources
fn fetch_grenadine() -> std::path::PathBuf {
    use sha2::{Digest, Sha256};
    // windows sets USERPROFILE, not HOME
    let home = env::var("HOME").or_else(|_| env::var("USERPROFILE")).unwrap();
    let cache_dir = Path::new(&home).join(".cache/choq/build");
    fs::create_dir_all(&cache_dir).unwrap();
    let jar = cache_dir.join(format!("grenadine-{}.jar", GRENADINE_VERSION));
    if !jar.exists() {
        let url = format!(
            "https://repo.clojars.org/cc/clojure/grenadine/{v}/grenadine-{v}.jar",
            v = GRENADINE_VERSION
        );
        println!("cargo:warning=downloading {}", url);
        let mut res = ureq::get(&url).call().expect("download grenadine jar");
        let mut bytes: Vec<u8> = Vec::new();
        use std::io::Read;
        res.body_mut()
            .as_reader()
            .read_to_end(&mut bytes)
            .expect("read grenadine jar");
        fs::write(&jar, &bytes).unwrap();
    }
    let bytes = fs::read(&jar).unwrap();
    let hex: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();
    if hex != GRENADINE_SHA256 {
        let _ = fs::remove_file(&jar);
        panic!("grenadine jar sha256 mismatch: {}", hex);
    }
    let dest = Path::new(&env::var("OUT_DIR").unwrap()).join("grenadine-src");
    let _ = fs::remove_dir_all(&dest);
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        let name = entry.name().to_string();
        if !(name.ends_with(".cljc") || name.ends_with(".cljs")) {
            continue;
        }
        let out = dest.join(&name);
        fs::create_dir_all(out.parent().unwrap()).unwrap();
        let mut content = String::new();
        use std::io::Read;
        entry.read_to_string(&mut content).unwrap();
        fs::write(out, content).unwrap();
    }
    for (patch_file, target) in PATCHES {
        // windows checkouts may introduce crlf, the jar sources are lf
        let patch_text = fs::read_to_string(patch_file).unwrap().replace("\r\n", "\n");
        let patch = diffy::Patch::from_str(&patch_text)
            .unwrap_or_else(|e| panic!("parsing {}: {}", patch_file, e));
        let target_path = dest.join(target);
        let base = fs::read_to_string(&target_path).unwrap();
        let patched = diffy::apply(&base, &patch)
            .unwrap_or_else(|e| panic!("applying {}: {}", patch_file, e));
        fs::write(target_path, patched).unwrap();
    }
    dest
}

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

// the quickjs stack limit is 12MB; the windows main thread only gets
// 1MB, so run the build on a thread with an explicit stack size
fn main() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(build)
        .expect("build thread")
        .join()
        .expect("build thread panicked");
}

fn build() {
    println!("cargo:rerun-if-changed=package.json");
    println!("cargo:rerun-if-changed=pnpm-lock.yaml");
    if !Path::new("node_modules/cherry-cljs").exists() {
        panic!("node_modules/cherry-cljs not found, run `pnpm install` first");
    }
    println!("cargo:rerun-if-changed=vendor");
    println!("cargo:rerun-if-changed=patches");
    println!("cargo:rerun-if-changed=src/choq");
    let grenadine_src = fetch_grenadine();
    let rt = Runtime::new().unwrap();
    rt.set_max_stack_size(12 * 1024 * 1024);
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
            let full = if path.starts_with("src/") {
                std::path::PathBuf::from(path)
            } else {
                grenadine_src.join(path)
            };
            let src = fs::read_to_string(&full)
                .unwrap_or_else(|e| panic!("{}: {}", full.display(), e));
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
