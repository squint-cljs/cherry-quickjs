use llrt_buffer::BufferModule;
use llrt_fs::{FsModule, FsPromisesModule};
use llrt_path::PathModule;
use llrt_timers::TimersModule;
use rquickjs::loader::{BuiltinResolver, Loader, ModuleLoader, Resolver};
use rquickjs::module::{Declared, Module};
use rquickjs::{
    async_with, AsyncContext, AsyncRuntime, CatchResultExt, Ctx, Error, Function, Promise,
};
use std::io::{BufRead, Write};

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
// and caches it under ~/.cache/cherry-quickjs, keyed by the url hash.
// Imports inside a remote module resolve against its url.

fn is_url(s: &str) -> bool {
    s.starts_with("https://") || s.starts_with("http://")
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
    let home = std::env::var("HOME").ok()?;
    let hex: String = Sha256::digest(url.as_bytes())
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();
    Some(std::path::Path::new(&home).join(".cache/cherry-quickjs").join(hex))
}

fn fetch_url(url: &str) -> Result<String, String> {
    if let Some(p) = url_cache_path(url) {
        if let Ok(cached) = std::fs::read_to_string(&p) {
            return Ok(cached);
        }
    }
    eprintln!("Downloading {}", url);
    let mut res = ureq::get(url).call().map_err(|e| e.to_string())?;
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
        if is_url(name) {
            return Ok(name.to_string());
        }
        if is_url(base) {
            return Ok(join_url(base, name));
        }
        let resolved = if name.starts_with("./") || name.starts_with("../") {
            let dir = base.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
            normalize(&format!("{}/{}", dir, name))
        } else {
            name.to_string()
        };
        if ASSETS.iter().any(|(n, _)| *n == resolved) {
            Ok(resolved)
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
    return ['error', __str(e), ns];
  }
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

async fn repl(ctx: &Ctx<'_>) {
    let stdin = std::io::stdin();
    let mut ns = "user".to_string();
    let mut buf = String::new();
    loop {
        if buf.is_empty() {
            print!("{}=> ", ns);
        } else {
            print!("      ");
        }
        std::io::stdout().flush().ok();
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
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

const NODE_MODULES: &[&str] = &[
    "fs",
    "node:fs",
    "fs/promises",
    "node:fs/promises",
    "path",
    "node:path",
    "buffer",
    "node:buffer",
    "timers",
    "node:timers",
];

fn main() {
    let exit_code = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
        .block_on(run());
    std::process::exit(exit_code);
}

async fn run() -> i32 {
    let rt = AsyncRuntime::new().expect("runtime");
    let mut builtin = BuiltinResolver::default();
    for name in NODE_MODULES {
        builtin = builtin.with_module(*name);
    }
    let modules = ModuleLoader::default()
        .with_module("fs", FsModule)
        .with_module("node:fs", FsModule)
        .with_module("fs/promises", FsPromisesModule)
        .with_module("node:fs/promises", FsPromisesModule)
        .with_module("path", PathModule)
        .with_module("node:path", PathModule)
        .with_module("buffer", BufferModule)
        .with_module("node:buffer", BufferModule)
        .with_module("timers", TimersModule)
        .with_module("node:timers", TimersModule);
    rt.set_loader((builtin, CherryResolver), (modules, CherryLoader))
        .await;
    let context = AsyncContext::full(&rt).await.expect("context");
    let exit_code = async_with!(context => |ctx| {
        llrt_buffer::init(&ctx).expect("buffer init");
        llrt_timers::init(&ctx).expect("timers init");
        llrt_fetch::init(&ctx).expect("fetch init");
        let print = Function::new(ctx.clone(), |s: String| println!("{}", s)).expect("print fn");
        ctx.globals().set("__print", print).expect("set __print");
        ctx.eval::<(), _>(CONSOLE_JS).expect("console setup");
        Module::evaluate(ctx.clone(), "bootstrap", BOOTSTRAP_JS)
            .expect("bootstrap declare")
            .into_future::<()>()
            .await
            .catch(&ctx)
            .expect("bootstrap eval");

        let args: Vec<String> = std::env::args().collect();
        if args.get(1).map(String::as_str) == Some("--version") {
            println!("cherry-quickjs {}", env!("CARGO_PKG_VERSION"));
            return 0;
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
                            0
                        }
                        _ => {
                            eprintln!("error: {}", payload);
                            1
                        }
                    }
                }
                None => {
                    eprintln!("usage: cherry-quickjs [-e expr]");
                    1
                }
            }
        } else {
            println!("Cherry QuickJS REPL, Ctrl-D to exit");
            repl(&ctx).await;
            0
        }
    })
    .await;
    rt.idle().await;
    exit_code
}
