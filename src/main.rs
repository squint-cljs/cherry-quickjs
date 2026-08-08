use rquickjs::function::Rest;
use rquickjs::loader::{ImportAttributes, Loader, Resolver};
use rquickjs::module::{Declared, Module};
use rquickjs::{CatchResultExt, Context, Ctx, Error, Exception, Function, Object, Promise, Runtime};
use std::cell::RefCell;
use std::io::{BufRead, Write};
use std::rc::Rc;

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

struct CherryResolver;

impl Resolver for CherryResolver {
    fn resolve<'js>(
        &mut self,
        _ctx: &Ctx<'js>,
        base: &str,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<String> {
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
        _attributes: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<Module<'js, Declared>> {
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

//// wasm plugins
//
// A plugin is a wasm module whose exported functions become members of the
// `plugin` global. A function with signature (i32, i32) -> i64 in a module
// that also exports `alloc` is called with a string: the host allocates,
// writes utf8 input and passes (ptr, len); the result i64 packs the output
// as (ptr << 32) | len. All other functions map their params and result
// to JS numbers.

type PluginStore = wasmi::Store<wasmi_wasi::WasiCtx>;

struct Plugin {
    store: RefCell<PluginStore>,
    instance: wasmi::Instance,
}

fn plugin_memory(p: &Plugin, store: &mut PluginStore) -> Result<wasmi::Memory, String> {
    p.instance
        .get_export(&mut *store, "memory")
        .and_then(wasmi::Extern::into_memory)
        .ok_or_else(|| "plugin exports no memory".to_string())
}

fn plugin_call_string(p: &Plugin, name: &str, input: &str) -> Result<String, String> {
    let store = &mut *p.store.borrow_mut();
    let alloc = p
        .instance
        .get_typed_func::<i32, i32>(&*store, "alloc")
        .map_err(|e| e.to_string())?;
    let ptr = alloc
        .call(&mut *store, input.len() as i32)
        .map_err(|e| e.to_string())?;
    let mem = plugin_memory(p, store)?;
    mem.write(&mut *store, ptr as usize, input.as_bytes())
        .map_err(|e| e.to_string())?;
    let f = p
        .instance
        .get_typed_func::<(i32, i32), i64>(&*store, name)
        .map_err(|e| e.to_string())?;
    let packed = f
        .call(&mut *store, (ptr, input.len() as i32))
        .map_err(|e| e.to_string())? as u64;
    let (rptr, rlen) = ((packed >> 32) as usize, (packed & 0xffff_ffff) as usize);
    let mut buf = vec![0u8; rlen];
    mem.read(&*store, rptr, &mut buf).map_err(|e| e.to_string())?;
    String::from_utf8(buf).map_err(|e| e.to_string())
}

fn plugin_call_numeric(p: &Plugin, name: &str, args: &[f64]) -> Result<Option<f64>, String> {
    let store = &mut *p.store.borrow_mut();
    let f = p
        .instance
        .get_export(&mut *store, name)
        .and_then(wasmi::Extern::into_func)
        .ok_or_else(|| format!("{} is not a function", name))?;
    let ty = f.ty(&*store);
    if ty.params().len() != args.len() {
        return Err(format!("{} expects {} arguments", name, ty.params().len()));
    }
    let inputs: Vec<wasmi::Val> = ty
        .params()
        .iter()
        .zip(args)
        .map(|(t, a)| match t {
            wasmi::ValType::I64 => wasmi::Val::I64(*a as i64),
            wasmi::ValType::F32 => wasmi::Val::F32((*a as f32).into()),
            wasmi::ValType::F64 => wasmi::Val::F64((*a).into()),
            _ => wasmi::Val::I32(*a as i32),
        })
        .collect();
    let mut outputs: Vec<wasmi::Val> = ty.results().iter().map(|t| wasmi::Val::default(*t)).collect();
    f.call(&mut *store, &inputs, &mut outputs)
        .map_err(|e| e.to_string())?;
    Ok(outputs.first().map(|v| match v {
        wasmi::Val::I32(x) => *x as f64,
        wasmi::Val::I64(x) => *x as f64,
        wasmi::Val::F32(x) => f32::from(*x) as f64,
        wasmi::Val::F64(x) => f64::from(*x),
        _ => f64::NAN,
    }))
}

fn throw<'js>(ctx: &Ctx<'js>, msg: &str) -> Error {
    Exception::throw_message(ctx, msg)
}

fn wasi_ctx(ctx: &Ctx<'_>, allow: &[String]) -> rquickjs::Result<wasmi_wasi::WasiCtx> {
    let mut builder = wasmi_wasi::WasiCtxBuilder::new();
    builder.inherit_stdout().inherit_stderr();
    for dir in allow {
        let handle = wasmi_wasi::Dir::open_ambient_dir(dir, wasmi_wasi::ambient_authority())
            .map_err(|e| throw(ctx, &format!("--allow {}: {}", dir, e)))?;
        builder
            .preopened_dir(handle, dir)
            .map_err(|e| throw(ctx, &e.to_string()))?;
    }
    Ok(builder.build())
}

fn load_plugins(ctx: &Ctx<'_>, paths: &[String], allow: &[String]) -> rquickjs::Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let plugin_obj = Object::new(ctx.clone())?;
    let engine = wasmi::Engine::default();
    for path in paths {
        let bytes = std::fs::read(path).map_err(|e| throw(ctx, &format!("{}: {}", path, e)))?;
        let module =
            wasmi::Module::new(&engine, &bytes).map_err(|e| throw(ctx, &e.to_string()))?;
        let has_alloc = module
            .exports()
            .any(|e| e.name() == "alloc" && matches!(e.ty(), wasmi::ExternType::Func(_)));
        let exports: Vec<(String, bool)> = module
            .exports()
            .filter_map(|e| match e.ty() {
                wasmi::ExternType::Func(ft) => {
                    let stringy = has_alloc
                        && ft.params()
                            == [wasmi::ValType::I32, wasmi::ValType::I32]
                        && ft.results() == [wasmi::ValType::I64];
                    Some((e.name().to_string(), stringy))
                }
                _ => None,
            })
            .collect();
        let mut store = wasmi::Store::new(&engine, wasi_ctx(ctx, allow)?);
        let mut linker = wasmi::Linker::<wasmi_wasi::WasiCtx>::new(&engine);
        wasmi_wasi::add_to_linker(&mut linker, |wasi| wasi)
            .map_err(|e| throw(ctx, &e.to_string()))?;
        let instance = linker
            .instantiate_and_start(&mut store, &module)
            .map_err(|e| throw(ctx, &e.to_string()))?;
        // wasi reactor modules export _initialize instead of a start section
        if let Ok(init) = instance.get_typed_func::<(), ()>(&store, "_initialize") {
            init.call(&mut store, ())
                .map_err(|e| throw(ctx, &e.to_string()))?;
        }
        let plugin = Rc::new(Plugin {
            store: RefCell::new(store),
            instance,
        });
        for (name, stringy) in exports {
            if name == "alloc" {
                continue;
            }
            let func = if stringy {
                let p = plugin.clone();
                let n = name.clone();
                Function::new(ctx.clone(), move |cx: Ctx<'_>, input: String| {
                    plugin_call_string(&p, &n, &input).map_err(|m| throw(&cx, &m))
                })?
            } else {
                let p = plugin.clone();
                let n = name.clone();
                Function::new(ctx.clone(), move |cx: Ctx<'_>, args: Rest<f64>| {
                    plugin_call_numeric(&p, &n, &args.0).map_err(|m| throw(&cx, &m))
                })?
            };
            plugin_obj.set(name.as_str(), func)?;
        }
    }
    ctx.globals().set("plugin", plugin_obj)?;
    Ok(())
}

fn eval_cherry(ctx: &Ctx<'_>, code: &str) -> (String, String, String) {
    let run = || -> rquickjs::Result<Vec<String>> {
        let f: Function = ctx.globals().get("__evalCherry")?;
        let p: Promise = f.call((code,))?;
        p.finish::<Vec<String>>()
    };
    match run().catch(ctx) {
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

fn repl(ctx: &Ctx<'_>) {
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
        let (status, payload, new_ns) = eval_cherry(ctx, &buf);
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

fn main() {
    let rt = Runtime::new().expect("runtime");
    rt.set_loader(CherryResolver, CherryLoader);
    let context = Context::full(&rt).expect("context");
    let exit_code = context.with(|ctx| {
        let print = Function::new(ctx.clone(), |s: String| println!("{}", s)).expect("print fn");
        ctx.globals().set("__print", print).expect("set __print");
        ctx.eval::<(), _>(CONSOLE_JS).expect("console setup");
        Module::evaluate(ctx.clone(), "bootstrap", BOOTSTRAP_JS)
            .expect("bootstrap declare")
            .finish::<()>()
            .catch(&ctx)
            .expect("bootstrap eval");

        let mut args: Vec<String> = std::env::args().skip(1).collect();
        let mut plugins: Vec<String> = Vec::new();
        let mut allow: Vec<String> = Vec::new();
        for (flag, target) in [("--plugin", &mut plugins), ("--allow", &mut allow)] {
            while let Some(i) = args.iter().position(|a| a == flag) {
                args.remove(i);
                if i < args.len() {
                    target.push(args.remove(i));
                } else {
                    eprintln!("{} needs an argument", flag);
                    return 1;
                }
            }
        }
        if args.first().map(String::as_str) == Some("--version") {
            println!("cherry-quickjs {}", env!("CARGO_PKG_VERSION"));
            return 0;
        }
        if let Err(e) = load_plugins(&ctx, &plugins, &allow).catch(&ctx) {
            eprintln!("error: {}", e);
            return 1;
        }
        if args.first().map(String::as_str) == Some("-e") {
            match args.get(1) {
                Some(code) => {
                    let (status, payload, _) = eval_cherry(&ctx, code);
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
                    eprintln!("usage: cherry-quickjs [--plugin file.wasm] [-e expr]");
                    1
                }
            }
        } else {
            println!("Cherry QuickJS REPL, Ctrl-D to exit");
            repl(&ctx);
            0
        }
    });
    std::process::exit(exit_code);
}
