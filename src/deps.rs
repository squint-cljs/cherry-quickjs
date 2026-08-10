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

// source roots added at runtime by choq.deps (library jars)
static SOURCE_ROOTS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

pub fn source_roots() -> Vec<String> {
    SOURCE_ROOTS.lock().unwrap().clone()
}

// namespaces load straight from jars, java-style: an entry index per
// jar answers resolution, entries decompress on demand
static JAR_INDEX: std::sync::Mutex<
    Option<std::collections::HashMap<String, std::sync::Arc<std::collections::HashSet<String>>>>,
> = std::sync::Mutex::new(None);

fn jar_entries(jar: &str) -> std::sync::Arc<std::collections::HashSet<String>> {
    let mut guard = JAR_INDEX.lock().unwrap();
    let map = guard.get_or_insert_with(Default::default);
    if let Some(entries) = map.get(jar) {
        return entries.clone();
    }
    let mut set = std::collections::HashSet::new();
    if let Ok(f) = std::fs::File::open(jar) {
        if let Ok(z) = zip::ZipArchive::new(f) {
            for name in z.file_names() {
                set.insert(name.to_string());
            }
        }
    }
    let entries = std::sync::Arc::new(set);
    map.insert(jar.to_string(), entries.clone());
    entries
}

// a namespace file inside one of the registered jar roots, as a
// jar:<path>!<entry> pseudo path
pub fn find_in_jar_roots(stem: &str) -> Option<String> {
    for root in source_roots() {
        if !root.ends_with(".jar") {
            continue;
        }
        for ext in ["cljs", "cljc"] {
            let entry = format!("{}.{}", stem, ext);
            if jar_entries(&root).contains(&entry) {
                return Some(format!("jar:{}!{}", root, entry));
            }
        }
    }
    None
}

fn read_jar_entry_fn<'js>(ctx: Ctx<'js>, jar: String, entry: String) -> rquickjs::Result<String> {
    read_entry(&jar, &entry)
        .map_err(|e| rquickjs::Exception::throw_message(&ctx, &format!("{} in {}: {}", entry, jar, e)))
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

// mvn:<group>/<artifact>@<version>/<some.ns> modules: ensure the
// dependency, compile the namespace, and emit a module with real
// exports. The loader is a host callback, so re-entrant ctx.eval into
// the engine is fine; the whole path is synchronous because the
// grenadine host is. Requires choq.deps to be loaded already (the
// repl and file paths preprocess instead; the nrepl boot imports it).
pub fn load_mvn<'js>(
    ctx: &Ctx<'js>,
    name: &str,
) -> rquickjs::Result<rquickjs::module::Module<'js, rquickjs::module::Declared>> {
    let err = |msg: String| rquickjs::Exception::throw_message(ctx, &msg);
    let bad = || err(format!("invalid mvn specifier: {}", name));
    let rest = name.strip_prefix("mvn:").unwrap();
    let (lib, rest) = rest.split_once('@').ok_or_else(bad)?;
    let (version, ns) = rest.split_once('/').ok_or_else(bad)?;
    if lib.is_empty() || version.is_empty() || ns.is_empty() {
        return Err(bad());
    }

    let loaded: bool = ctx.eval("globalThis.choq?.deps != null")?;
    if !loaded {
        return Err(err(
            "mvn: requires choq.deps; run (require '[choq.deps]) first".into(),
        ));
    }
    ctx.eval::<rquickjs::Value, _>(format!(
        "globalThis.choq.deps.add_mvn_dep({:?}, {:?})",
        lib, version
    ))?;

    let stem = ns.replace('.', "/").replace('-', "_");
    let jar_path = find_in_jar_roots(&stem)
        .ok_or_else(|| err(format!("namespace {} not found in {}", ns, lib)))?;
    let bang = jar_path.find('!').unwrap();
    let src = read_entry(&jar_path[4..bang], &jar_path[bang + 1..])
        .map_err(|e| err(format!("reading {}: {}", jar_path, e)))?;

    // compiled-output cache, same layout as __evalCherryFile's
    use sha2::{Digest, Sha256};
    let sha: String = Sha256::digest(src.as_bytes())
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();
    let cache_dir =
        std::path::Path::new(&crate::home_dir().unwrap()).join(".cache/choq/compiled");
    let cache = cache_dir.join(format!("{}.js", sha));
    let js = if let Ok(cached) = std::fs::read_to_string(&cache) {
        cached
    } else {
        ctx.globals().set("__mvnSrc", src)?;
        let compiled: String = ctx.eval("__compileCherry(globalThis.__mvnSrc)")?;
        let _ = std::fs::create_dir_all(&cache_dir);
        let _ = std::fs::write(&cache, &compiled);
        compiled
    };

    // exports are the vars the compiled output assigns on the ns object
    let munged_ns = ns.replace('-', "_");
    let assign_prefix = format!("globalThis.{}.", munged_ns);
    let mut vars: std::collections::BTreeSet<String> = Default::default();
    for chunk in js.split(&assign_prefix).skip(1) {
        let ident: String = chunk
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
            .collect();
        if !ident.is_empty() && chunk[ident.len()..].trim_start().starts_with('=') {
            vars.insert(ident);
        }
    }

    let mut module = format!(
        "await (async function () {{\n{}\n}})();\nconst __n = globalThis.{};\n",
        js, munged_ns
    );
    for v in &vars {
        module.push_str(&format!("export const {} = __n.{};\n", v, v));
    }
    rquickjs::module::Module::declare(ctx.clone(), name, module)
}

fn read_entry(jar: &str, entry: &str) -> Result<String, String> {
    let f = std::fs::File::open(jar).map_err(|e| e.to_string())?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
    let mut file = z.by_name(entry).map_err(|e| e.to_string())?;
    let mut out = String::new();
    use std::io::Read;
    file.read_to_string(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

// sync http for the grenadine host map; body as Uint8Array
fn http_get_sync_fn<'js>(ctx: Ctx<'js>, url: String) -> rquickjs::Result<rquickjs::Object<'js>> {
    let obj = rquickjs::Object::new(ctx.clone())?;
    eprintln!("Downloading {}", url);
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
    let read_jar = Function::new(ctx.clone(), read_jar_entry_fn).expect("read_jar fn");
    ctx.globals()
        .set("__readJarEntry", read_jar)
        .expect("set __readJarEntry");
}
