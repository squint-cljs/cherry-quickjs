// choq.deps: Clojure dependency support. Vendored grenadine plus the
// choq host namespace, compiled to js by build.rs and served as
// modules. Only choq.deps is visible to user code; the internals
// resolve solely between precompiled modules.

use rquickjs::{Ctx, Function};

macro_rules! cljc {
    ($ns:literal) => {
        ($ns, include_str!(concat!(env!("OUT_DIR"), "/cljc.", $ns, ".js")))
    };
}

const PRECOMPILED: &[(&str, &str)] = &[
    cljc!("grenadine.version"),
    cljc!("grenadine.xml"),
    cljc!("grenadine.expander"),
    cljc!("grenadine.gitlibs"),
    cljc!("grenadine.source"),
    cljc!("grenadine.pom"),
    cljc!("grenadine.lock"),
    cljc!("grenadine.repo"),
    cljc!("grenadine.coordinate"),
    cljc!("grenadine.graph"),
    cljc!("grenadine.basis"),
    cljc!("grenadine.core"),
    cljc!("grenadine.runtime"),
    cljc!("choq.deps"),
    cljc!("clojurestar.deps"),
];

const PUBLIC: &[&str] = &["choq.deps"];

// source roots added at runtime by choq.deps (extracted library jars)
static SOURCE_ROOTS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

pub fn source_roots() -> Vec<String> {
    SOURCE_ROOTS.lock().unwrap().clone()
}

pub fn resolve(base: &str, name: &str) -> Option<String> {
    let (found, _) = PRECOMPILED.iter().find(|(n, _)| *n == name)?;
    let internal_base = PRECOMPILED.iter().any(|(n, _)| *n == base);
    if PUBLIC.contains(found) || internal_base {
        Some(found.to_string())
    } else {
        None
    }
}

// the compiled js is repl-style output; an async iife makes it a module
pub fn load<'js>(
    ctx: &Ctx<'js>,
    name: &str,
) -> Option<rquickjs::Result<rquickjs::module::Module<'js, rquickjs::module::Declared>>> {
    let (_, js) = PRECOMPILED.iter().find(|(n, _)| *n == name)?;
    let wrapped = format!("await (async function () {{\n{}\n}})();", js);
    Some(rquickjs::module::Module::declare(ctx.clone(), name, wrapped))
}

// sync http for the grenadine host map; body as Uint8Array
fn http_get_sync_fn<'js>(ctx: Ctx<'js>, url: String) -> rquickjs::Result<rquickjs::Object<'js>> {
    let obj = rquickjs::Object::new(ctx.clone())?;
    match ureq::get(&url).header("user-agent", "choq").call() {
        Ok(mut res) => {
            let status = res.status().as_u16();
            let mut body: Vec<u8> = Vec::new();
            use std::io::Read;
            res.body_mut().as_reader().read_to_end(&mut body).ok();
            obj.set("status", status)?;
            obj.set("body", rquickjs::TypedArray::new(ctx.clone(), body)?)?;
        }
        Err(_) => {
            obj.set("status", 0u16)?;
        }
    }
    Ok(obj)
}

fn add_source_roots_fn(roots: Vec<String>) {
    SOURCE_ROOTS.lock().unwrap().extend(roots);
}

pub fn init(ctx: &Ctx<'_>) {
    let http_get_sync = Function::new(ctx.clone(), http_get_sync_fn).expect("http_get_sync fn");
    ctx.globals()
        .set("__httpGetSync", http_get_sync)
        .expect("set __httpGetSync");
    let add_roots = Function::new(ctx.clone(), add_source_roots_fn).expect("add_roots fn");
    ctx.globals()
        .set("__addSourceRoots", add_roots)
        .expect("set __addSourceRoots");
}
