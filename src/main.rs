use llrt_buffer::BufferModule;
use llrt_crypto::CryptoModule;
use llrt_fs::{FsModule, FsPromisesModule};
use llrt_net::NetModule;
use llrt_os::OsModule;
use llrt_path::PathModule;
use llrt_process::ProcessModule;
use llrt_timers::TimersModule;
use llrt_tty::TtyModule;
use llrt_zlib::ZlibModule;
use rquickjs::loader::{BuiltinResolver, Loader, ModuleLoader, Resolver};
use rquickjs::module::{Declared, Module};
use rquickjs::{
    async_with, AsyncContext, AsyncRuntime, CatchResultExt, Ctx, Error, Function, Promise,
};
use std::io::Write;

mod deps;
mod serve;

macro_rules! bytecode {
    ($file:literal) => {
        include_bytes!(concat!(env!("OUT_DIR"), "/", $file))
    };
}

// modules precompiled to quickjs bytecode by build.rs
const ASSETS: &[(&str, &[u8])] = &[
    ("cherry-cljs/cljs.core.js", bytecode!("cherry-cljs_cljs.core.js")),
    ("cherry-cljs/lib/cljs.core.js", bytecode!("cherry-cljs_lib_cljs.core.js")),
    ("cherry-cljs/lib/clojure.string.js", bytecode!("cherry-cljs_lib_clojure.string.js")),
    ("cherry-cljs/lib/clojure.set.js", bytecode!("cherry-cljs_lib_clojure.set.js")),
    ("cherry-cljs/lib/clojure.walk.js", bytecode!("cherry-cljs_lib_clojure.walk.js")),
    ("cherry-cljs/lib/cljs.pprint.js", bytecode!("cherry-cljs_lib_cljs.pprint.js")),
    ("cherry-cljs/lib/clojure.test.js", bytecode!("cherry-cljs_lib_clojure.test.js")),
    ("cherry-cljs/lib/compiler.js", bytecode!("cherry-cljs_lib_compiler.js")),
    ("cherry-cljs/lib/compiler.node.js", bytecode!("cherry-cljs_lib_compiler.node.js")),
    ("cherry-cljs/lib/node.js", bytecode!("cherry-cljs_lib_node.js")),
    ("cherry-cljs/lib/node.nrepl_server.js", bytecode!("cherry-cljs_lib_node.nrepl_server.js")),
];

// llrt_fs lacks existsSync; the native module registers as llrt:fs and
// this wrapper fills the gap
const FS_WRAPPER_JS: &str = r#"
import * as fs from 'llrt:fs';
export * from 'llrt:fs';
export const existsSync = (p) => { try { fs.accessSync(p); return true; } catch (e) { return false; } };
export default Object.assign({}, fs.default, { existsSync });
"#;

// node:stream is the vendored readable-stream bundle; its internal
// require('stream') resolves back here lazily, so the cycle is safe
const STREAM_JS: &str = r#"
export * from 'vendor:readable-stream';
export { default } from 'vendor:readable-stream';
"#;

// js-implemented builtins; empty source = import-satisfying stub
const JS_BUILTINS: &[(&str, &str)] = &[
    ("child_process", ""),
    ("node:child_process", ""),
    ("fs", FS_WRAPPER_JS),
    ("node:fs", FS_WRAPPER_JS),
    ("stream", STREAM_JS),
    ("node:stream", STREAM_JS),
    ("vendor:readable-stream", include_str!("../vendor/readable-stream.mjs")),
    ("events", include_str!("../vendor/events.mjs")),
    ("node:events", include_str!("../vendor/events.mjs")),
    ("string_decoder", include_str!("../vendor/string_decoder.mjs")),
    ("node:string_decoder", include_str!("../vendor/string_decoder.mjs")),
];

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

//// url imports
//
// https:// specifiers load like in deno: the loader downloads the module
// and caches it under ~/.cache/choq, keyed by the url hash.
// Imports inside a remote module resolve against its url.

// windows sets USERPROFILE, not HOME
pub fn home_dir() -> Option<String> {
    std::env::var("HOME").ok().or_else(|| std::env::var("USERPROFILE").ok())
}

fn is_url(s: &str) -> bool {
    s.starts_with("https://") || s.starts_with("http://")
}

// local namespaces: a bare ns specifier resolves to a .cljs or .cljc
// file on the conventional paths or a registered source root; loaded
// through a shim that compiles it in-engine
fn local_cljs_path(name: &str) -> Option<String> {
    if name.contains('/') || name.contains(':') || name.starts_with('.') {
        return None;
    }
    let stem = name.replace('.', "/").replace('-', "_");
    let mut dirs: Vec<String> = vec!["".into(), "src/".into(), "test/".into()];
    for root in deps::source_roots() {
        if !root.ends_with(".jar") {
            dirs.push(format!("{}/", root));
        }
    }
    if let Some(p) = deps::find_in_jar_roots(&stem) {
        return Some(p);
    }
    for dir in &dirs {
        for ext in ["cljs", "cljc"] {
            let p = format!("{}{}.{}", dir, stem, ext);
            if std::path::Path::new(&p).is_file() {
                return Some(p);
            }
        }
    }
    None
}

fn join_url(base: &str, name: &str) -> String {
    if is_url(name) {
        return name.to_string();
    }
    let scheme_end = base.find("://").map(|i| i + 3).unwrap_or(0);
    let host_end = base[scheme_end..]
        .find('/')
        .map(|i| scheme_end + i)
        .unwrap_or(base.len());
    if let Some(rest) = name.strip_prefix('/') {
        format!("{}/{}", &base[..host_end], rest)
    } else {
        let dir = base.rsplit_once('/').map(|(d, _)| d).unwrap_or(base);
        format!("{}/{}", &base[..host_end], normalize(&format!("{}/{}", &dir[host_end..], name)))
    }
}

fn url_cache_path(url: &str) -> Option<std::path::PathBuf> {
    use sha2::{Digest, Sha256};
    let home = home_dir()?;
    let hex: String = Sha256::digest(url.as_bytes())
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();
    Some(std::path::Path::new(&home).join(".cache/choq").join(hex))
}

fn fetch_url(url: &str) -> Result<String, String> {
    if let Some(p) = url_cache_path(url) {
        if let Ok(cached) = std::fs::read_to_string(&p) {
            return Ok(cached);
        }
    }
    eprintln!("Downloading {}", url);
    // a node-ish user agent makes esm.sh serve node builds, whose builtin
    // imports (node:tty etc.) map to our native modules
    let mut res = ureq::get(url)
        .header("user-agent", "Node.js/22.0.0")
        .call()
        .map_err(|e| e.to_string())?;
    let body = res
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    if let Some(p) = url_cache_path(url) {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&p, &body);
    }
    Ok(body)
}

struct CherryResolver;

impl Resolver for CherryResolver {
    fn resolve<'js>(
        &mut self,
        _ctx: &Ctx<'js>,
        base: &str,
        name: &str,
    ) -> rquickjs::Result<String> {
        if serve::JS_MODULES
            .iter()
            .chain(JS_BUILTINS)
            .any(|(n, _)| *n == name)
        {
            return Ok(name.to_string());
        }
        if is_url(name) || is_url(base) {
            // a remote module importing a node builtin directly
            if NODE_MODULES.contains(&name) {
                return Ok(name.to_string());
            }
            let url = if is_url(name) {
                name.to_string()
            } else {
                join_url(base, name)
            };
            // esm.sh serves node builtins as /node/<name>.mjs browser shims;
            // serve our native modules instead when we have them
            if let Some(rest) = url.split("://").nth(1) {
                if let Some(slash) = rest.find('/') {
                    let path = &rest[slash..];
                    if let Some(stem) = path
                        .strip_prefix("/node/")
                        .and_then(|p| p.strip_suffix(".mjs"))
                    {
                        let builtin = format!("node:{}", stem);
                        if NODE_MODULES.contains(&builtin.as_str()) {
                            return Ok(builtin);
                        }
                    }
                }
            }
            return Ok(url);
        }
        if name.starts_with("mvn:") {
            return Ok(name.to_string());
        }
        let resolved = if name.starts_with("./") || name.starts_with("../") {
            let dir = base.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
            normalize(&format!("{}/{}", dir, name))
        } else {
            name.to_string()
        };
        if ASSETS.iter().any(|(n, _)| *n == resolved) {
            Ok(resolved)
        } else if let Some(n) = deps::resolve(base, &resolved) {
            Ok(n)
        } else if let Some(p) = local_cljs_path(&resolved) {
            Ok(format!("cljs:{}", p))
        } else {
            Err(Error::new_resolving(base, name))
        }
    }
}

struct CherryLoader;

impl Loader for CherryLoader {
    fn load<'js>(
        &mut self,
        ctx: &Ctx<'js>,
        name: &str,
    ) -> rquickjs::Result<Module<'js, Declared>> {
        if let Some((_, src)) = serve::JS_MODULES
            .iter()
            .chain(JS_BUILTINS)
            .find(|(n, _)| *n == name)
        {
            return Module::declare(ctx.clone(), name, *src);
        }
        if let Some(m) = deps::load(&ctx, name) {
            return m;
        }
        if let Some(path) = name.strip_prefix("cljs:") {
            let shim = format!("await globalThis.__evalCherryFile({:?});", path);
            return Module::declare(ctx.clone(), name, shim);
        }
        if name.starts_with("mvn:") {
            return deps::load_mvn(ctx, name);
        }
        if is_url(name) {
            let src = fetch_url(name)
                .map_err(|e| Error::new_loading_message(name, e))?;
            return Module::declare(ctx.clone(), name, src);
        }
        let bytes = ASSETS
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, b)| *b)
            .ok_or_else(|| Error::new_loading(name))?;
        unsafe { Module::load(ctx.clone(), bytes) }
    }
}

// llrt Buffer.indexOf only supports number needles; add string/Buffer
const POLYFILL_JS: &str = r#"
globalThis.global = globalThis;
if (!process.nextTick) {
  process.nextTick = (fn, ...args) => queueMicrotask(() => fn(...args));
}
{
  const orig = Buffer.prototype.indexOf;
  Buffer.prototype.indexOf = function (needle, offset, encoding) {
    if (typeof needle === 'number') return orig.call(this, needle, offset, encoding);
    const n = typeof needle === 'string'
      ? Buffer.from(needle, typeof offset === 'string' ? offset : encoding)
      : needle;
    const start = typeof offset === 'number' ? Math.max(0, offset) : 0;
    if (n.length === 0) return Math.min(start, this.length);
    outer: for (let i = start; i <= this.length - n.length; i++) {
      for (let j = 0; j < n.length; j++) if (this[i + j] !== n[j]) continue outer;
      return i;
    }
    return -1;
  };
}
"#;

const CONSOLE_JS: &str = r#"
globalThis.console = {
  log: (...args) => __print(args.map(__str).join(' ')),
  info: (...args) => __print(args.map(__str).join(' ')),
  warn: (...args) => __print(args.map(__str).join(' ')),
  error: (...args) => __print(args.map(__str).join(' ')),
  debug: (...args) => __print(args.map(__str).join(' ')),
};
globalThis.__str = (x) => {
  try { return String(x); }
  catch (_e) { return Object.prototype.toString.call(x); }
};
"#;

const BOOTSTRAP_JS: &str = r#"
import * as compiler from 'cherry-cljs/lib/compiler.js';
import * as core from 'cherry-cljs/cljs.core.js';
const st = { state: null };
globalThis.__evalCherry = async (code) => {
  // mvn: modules are served by the loader, which is sync and cannot
  // pull in choq.deps itself; load it up front when the source hints
  if (code.includes('mvn:') && globalThis.choq?.deps == null) {
    try { await import('choq.deps'); } catch (e) { return ['error', __str(e), 'user']; }
  }
  let res;
  try {
    res = compiler.compileStringEx(code, {repl: true, context: 'return', elide_exports: true}, st.state);
  } catch (e) {
    const m = String((e && e.message) || e);
    if (m.includes('EOF while reading')) return ['incomplete', '', ''];
    return ['compile-error', m, ''];
  }
  st.state = res;
  const ns = res.ns ? String(res.ns) : 'user';
  try {
    const v = await (0, eval)('(async function () {\n' + res.javascript + '\n})()');
    return ['ok', (v === null || v === undefined) ? 'nil' : core.pr_str(v), ns];
  } catch (e) {
    // *e, like a Clojure repl: cljs.core/*e is a dynamic var box
    if (core._STAR_e) core._STAR_e.val = e;
    return ['error', __str(e), ns];
  }
};
globalThis.__compileCherry = (src) => {
  const res = compiler.compileStringEx(src, {repl: true, context: 'return', elide_exports: true}, st.state);
  st.state = res;
  return res.javascript;
};
globalThis.__evalCherryFile = async (path) => {
  const fs = await import('fs');
  const crypto = await import('crypto');
  const os = await import('os');
  let src;
  if (path.startsWith('jar:')) {
    const bang = path.indexOf('!');
    src = __readJarEntry(path.slice(4, bang), path.slice(bang + 1));
  } else {
    src = fs.readFileSync(path, 'utf8');
  }
  // compiled-output cache keyed on the source
  const sha = crypto.createHash('sha256').update(src).digest('hex');
  const dir = os.homedir() + '/.cache/choq/compiled';
  const cached = dir + '/' + sha + '.js';
  let js;
  if (fs.existsSync(cached)) {
    js = fs.readFileSync(cached, 'utf8');
  } else {
    const res = compiler.compileStringEx(src, {repl: true, context: 'return', elide_exports: true}, st.state);
    st.state = res;
    js = res.javascript;
    fs.mkdirSync(dir, {recursive: true});
    fs.writeFileSync(cached, js);
  }
  await (0, eval)('(async function () {\n' + js + '\n})()');
};
"#;

async fn eval_cherry(ctx: &Ctx<'_>, code: &str) -> (String, String, String) {
    let run = async {
        let f: Function = ctx.globals().get("__evalCherry")?;
        let p: Promise = f.call((code,))?;
        p.into_future::<Vec<String>>().await
    };
    match run.await.catch(ctx) {
        Ok(v) => {
            let mut it = v.into_iter();
            (
                it.next().unwrap_or_default(),
                it.next().unwrap_or_default(),
                it.next().unwrap_or_else(|| "user".into()),
            )
        }
        Err(e) => ("error".into(), e.to_string(), "user".into()),
    }
}

// stdin reads run on a blocking thread so server tasks stay live
// between inputs
async fn repl(ctx: &Ctx<'_>) {
    let mut ns = "user".to_string();
    let mut buf = String::new();
    loop {
        if buf.is_empty() {
            print!("{}=> ", ns);
        } else {
            print!("      ");
        }
        std::io::stdout().flush().ok();
        let line = match tokio::task::spawn_blocking(|| {
            let mut s = String::new();
            std::io::stdin().read_line(&mut s).map(|n| (n, s))
        })
        .await
        {
            Ok(Ok((0, _))) => return,
            Ok(Ok((_, s))) => s,
            _ => return,
        };
        if buf.is_empty() && line.trim().is_empty() {
            continue;
        }
        buf.push_str(&line);
        let (status, payload, new_ns) = eval_cherry(ctx, &buf).await;
        match status.as_str() {
            "incomplete" => continue,
            "ok" => {
                println!("{}", payload);
                ns = new_ns;
            }
            "compile-error" => println!("compile error: {}", payload),
            _ => println!("error: {}", payload),
        }
        buf.clear();
    }
}

// a script that bound a listener keeps the process alive, node-style
async fn wait_for_servers(listeners: &std::cell::Cell<usize>) {
    if listeners.get() > 0 {
        std::future::pending::<()>().await;
    }
}

const NODE_MODULES: &[&str] = &[
    "llrt:fs",
    "fs/promises",
    "node:fs/promises",
    "path",
    "node:path",
    "buffer",
    "node:buffer",
    "timers",
    "node:timers",
    "tty",
    "node:tty",
    "crypto",
    "node:crypto",
    "net",
    "node:net",
    "os",
    "node:os",
    "process",
    "node:process",
    "zlib",
    "node:zlib",
];

fn main() {
    // the quickjs stack limit is 4MB; the windows main thread only gets
    // 1MB, so run everything on a thread with an explicit stack size
    let exit_code = std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            let local = tokio::task::LocalSet::new();
            local.block_on(&rt, run())
        })
        .expect("main thread")
        .join()
        .expect("main thread panicked");
    std::process::exit(exit_code);
}

async fn run() -> i32 {
    let rt = AsyncRuntime::new().expect("runtime");
    // the cherry compiler recurses deeply on nested forms, the quickjs
    // default stack limit is too small for it
    rt.set_max_stack_size(4 * 1024 * 1024).await;
    let mut builtin = BuiltinResolver::default();
    for name in NODE_MODULES {
        builtin = builtin.with_module(*name);
    }
    let modules = ModuleLoader::default()
        .with_module("llrt:fs", FsModule)
        .with_module("fs/promises", FsPromisesModule)
        .with_module("node:fs/promises", FsPromisesModule)
        .with_module("path", PathModule)
        .with_module("node:path", PathModule)
        .with_module("buffer", BufferModule)
        .with_module("node:buffer", BufferModule)
        .with_module("timers", TimersModule)
        .with_module("node:timers", TimersModule)
        .with_module("tty", TtyModule)
        .with_module("node:tty", TtyModule)
        .with_module("crypto", CryptoModule)
        .with_module("node:crypto", CryptoModule)
        .with_module("net", NetModule)
        .with_module("node:net", NetModule)
        .with_module("os", OsModule)
        .with_module("node:os", OsModule)
        .with_module("process", ProcessModule)
        .with_module("node:process", ProcessModule)
        .with_module("zlib", ZlibModule)
        .with_module("node:zlib", ZlibModule);
    rt.set_loader((builtin, CherryResolver), (modules, CherryLoader))
        .await;
    let context = AsyncContext::full(&rt).await.expect("context");

    let serve_tx = serve::start(context.clone());
    let listeners = std::rc::Rc::new(std::cell::Cell::new(0usize));

    let listeners_after = listeners.clone();
    let exit_code = async_with!(context => |ctx| {
        llrt_buffer::init(&ctx).expect("buffer init");
        llrt_timers::init(&ctx).expect("timers init");
        llrt_fetch::init(&ctx).expect("fetch init");
        llrt_crypto::init(&ctx).expect("crypto init");
        llrt_process::init(&ctx).expect("process init");
        let print = Function::new(ctx.clone(), |s: String| println!("{}", s)).expect("print fn");
        ctx.globals().set("__print", print).expect("set __print");
        serve::init(&ctx, serve_tx, listeners.clone());
        deps::init(&ctx);
        ctx.eval::<(), _>(POLYFILL_JS).expect("polyfill setup");
        ctx.eval::<(), _>(CONSOLE_JS).expect("console setup");
        Module::evaluate(ctx.clone(), "bootstrap", BOOTSTRAP_JS)
            .expect("bootstrap declare")
            .into_future::<()>()
            .await
            .catch(&ctx)
            .expect("bootstrap eval");

        let args: Vec<String> = std::env::args().collect();
        if args.get(1).map(String::as_str) == Some("--version") {
            println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
            return 0;
        }
        if matches!(args.get(1).map(String::as_str), Some("--help") | Some("-h")) {
            println!("usage: choq [-e expr] [--nrepl [port]] [file]");
            return 0;
        }
        if args.get(1).map(String::as_str) == Some("--nrepl") {
            // explicit port is used as given; the default falls back to a
            // free port when 1339 is taken
            let port: u16 = match args.get(2).and_then(|p| p.parse().ok()) {
                Some(p) => p,
                None => match std::net::TcpListener::bind(("127.0.0.1", 1339)) {
                    Ok(_) => 1339,
                    Err(_) => 0,
                },
            };
            let boot = format!(
                "(async () => {{ \
                   const net = await import('net'); \
                   if (net.Socket && !net.Socket.prototype.setNoDelay) \
                     net.Socket.prototype.setNoDelay = function () {{ return this; }}; \
                   await import('choq.deps'); \
                   const m = await import('cherry-cljs/lib/node.nrepl_server.js'); \
                   await m.startServer({{port: {}}}); \
                 }})()",
                port
            );
            let run = async {
                let p: Promise = ctx.eval(boot)?;
                p.into_future::<()>().await
            };
            return match run.await.catch(&ctx) {
                Ok(()) => {
                    std::future::pending::<()>().await;
                    0
                }
                Err(e) => {
                    eprintln!("error: {}", e);
                    1
                }
            };
        }
        if args.get(1).map(String::as_str) == Some("-e") {
            match args.get(2) {
                Some(code) => {
                    let (status, payload, _) = eval_cherry(&ctx, code).await;
                    match status.as_str() {
                        "ok" => {
                            if payload != "nil" {
                                println!("{}", payload);
                            }
                            wait_for_servers(&listeners_after).await;
                            0
                        }
                        _ => {
                            eprintln!("error: {}", payload);
                            1
                        }
                    }
                }
                None => {
                    eprintln!("usage: choq [-e expr] [--nrepl [port]] [file]");
                    1
                }
            }
        } else if let Some(file) = args.get(1) {
            if file.starts_with('-') {
                eprintln!("unknown option: {}", file);
                eprintln!("usage: choq [-e expr] [--nrepl [port]] [file]");
                return 1;
            }
            match std::fs::read_to_string(file) {
                Ok(code) => {
                    let (status, payload, _) = eval_cherry(&ctx, &code).await;
                    match status.as_str() {
                        "ok" => {
                            wait_for_servers(&listeners_after).await;
                            0
                        }
                        _ => {
                            eprintln!("error: {}", payload);
                            1
                        }
                    }
                }
                Err(e) => {
                    eprintln!("error: {}: {}", file, e);
                    1
                }
            }
        } else {
            println!("Choq REPL, Ctrl-D to exit");
            repl(&ctx).await;
            0
        }
    })
    .await;
    rt.idle().await;
    exit_code
}
