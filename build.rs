use rquickjs::loader::{ImportAttributes, Loader, Resolver};
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
        _attributes: Option<ImportAttributes<'js>>,
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
        _attributes: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<Module<'js, Declared>> {
        declare_and_write(ctx, name)
    }
}

fn main() {
    println!("cargo:rerun-if-changed=package.json");
    println!("cargo:rerun-if-changed=pnpm-lock.yaml");
    if !Path::new("node_modules/cherry-cljs").exists() {
        panic!("node_modules/cherry-cljs not found, run `pnpm install` first");
    }
    let rt = Runtime::new().unwrap();
    rt.set_loader(AssetResolver, AssetLoader);
    let context = Context::full(&rt).unwrap();
    context.with(|ctx| {
        for name in ROOTS {
            if let Err(e) = declare_and_write(&ctx, name) {
                panic!("declaring {}: {} {:?}", name, e, ctx.catch());
            }
        }
    });
}
