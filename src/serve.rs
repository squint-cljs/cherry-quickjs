// choq.http/serve: hyper accepts connections and hands requests to
// the js context over a channel, the response travels back over a oneshot

use rquickjs::{async_with, AsyncContext, CatchResultExt, Ctx, Error, Exception, Function, Promise, TypedArray};
use std::cell::Cell;
use std::rc::Rc;

pub struct ServeRequest {
    port: u16,
    method: String,
    url: String,
    headers: Vec<Vec<String>>,
    body: Vec<u8>,
    resp: tokio::sync::oneshot::Sender<(u16, Vec<Vec<String>>, Vec<u8>)>,
}

pub type ServeSender = tokio::sync::mpsc::UnboundedSender<ServeRequest>;

const CHOQ_HTTP_JS: &str = r#"
import * as core from 'cherry-cljs/cljs.core.js';
const portk = core.keyword('port');
export function serve(handler, opts) {
  let port = null;
  if (opts != null) {
    port = core.get(opts, portk);
    if (port == null && opts.port != null) port = opts.port;
  }
  if (port == null) port = 3000;
  globalThis.__serveHandlers[port] = handler;
  __listen(port);
}
// cherry resolves (require '[choq.http ...]) through globalThis
globalThis.choq = globalThis.choq || {};
globalThis.choq.http = { serve };
"#;

pub const JS_MODULES: &[(&str, &str)] = &[
    ("choq.http", CHOQ_HTTP_JS),
    ("choq:http", CHOQ_HTTP_JS),
];

const GLUE_JS: &str = r#"
globalThis.__serveHandlers = {};
globalThis.__handleRequest = async (port, method, url, headers, body) => {
  const init = { method, headers };
  if (method !== 'GET' && method !== 'HEAD' && body.length > 0) init.body = body;
  const res = await globalThis.__serveHandlers[port](new Request(url, init));
  const buf = new Uint8Array(await res.arrayBuffer());
  return [res.status, [...res.headers.entries()], buf];
};
"#;

// spawns the dispatcher that runs js handlers for incoming requests
pub fn start(context: AsyncContext) -> ServeSender {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<ServeRequest>();
    tokio::task::spawn_local(async move {
        while let Some(req) = rx.recv().await {
            tokio::task::spawn_local(js_handle(context.clone(), req));
        }
    });
    tx
}

// registers __listen and the request glue, bumps listeners on each bind
pub fn init(ctx: &Ctx<'_>, tx: ServeSender, listeners: Rc<Cell<usize>>) {
    let listen = Function::new(ctx.clone(), move |ctx: Ctx<'_>, port: u16| {
        let listener = std::net::TcpListener::bind(("127.0.0.1", port))
            .and_then(|l| {
                l.set_nonblocking(true)?;
                tokio::net::TcpListener::from_std(l)
            })
            .map_err(|e| {
                Exception::throw_message(&ctx, &format!("listen on port {}: {}", port, e))
            })?;
        listeners.set(listeners.get() + 1);
        eprintln!("Listening on http://localhost:{}", port);
        let tx = tx.clone();
        tokio::task::spawn_local(async move {
            loop {
                if let Ok((stream, _)) = listener.accept().await {
                    tokio::task::spawn_local(serve_connection(stream, tx.clone(), port));
                }
            }
        });
        Ok::<_, Error>(())
    })
    .expect("listen fn");
    ctx.globals().set("__listen", listen).expect("set __listen");
    ctx.eval::<(), _>(GLUE_JS).expect("serve glue setup");
}

async fn serve_connection(stream: tokio::net::TcpStream, tx: ServeSender, port: u16) {
    use hyper::service::service_fn;
    let io = hyper_util::rt::TokioIo::new(stream);
    let service = service_fn(move |req| {
        let tx = tx.clone();
        async move { hyper_to_js(req, tx, port).await }
    });
    let _ = hyper::server::conn::http1::Builder::new()
        .serve_connection(io, service)
        .await;
}

async fn hyper_to_js(
    req: hyper::Request<hyper::body::Incoming>,
    tx: ServeSender,
    port: u16,
) -> Result<hyper::Response<http_body_util::Full<bytes::Bytes>>, std::convert::Infallible> {
    use http_body_util::{BodyExt, Full};
    let (parts, body) = req.into_parts();
    let body = match body.collect().await {
        Ok(b) => b.to_bytes().to_vec(),
        Err(_) => Vec::new(),
    };
    let host = parts
        .headers
        .get(hyper::header::HOST)
        .and_then(|h| h.to_str().ok())
        .map(String::from)
        .unwrap_or_else(|| format!("localhost:{}", port));
    let url = format!("http://{}{}", host, parts.uri);
    let headers = parts
        .headers
        .iter()
        .map(|(k, v)| vec![k.to_string(), String::from_utf8_lossy(v.as_bytes()).into_owned()])
        .collect();
    let (otx, orx) = tokio::sync::oneshot::channel();
    let sent = tx
        .send(ServeRequest {
            port,
            method: parts.method.to_string(),
            url,
            headers,
            body,
            resp: otx,
        })
        .is_ok();
    let error_response = || {
        hyper::Response::builder()
            .status(500)
            .body(Full::new(bytes::Bytes::from_static(b"internal error")))
            .unwrap()
    };
    if !sent {
        return Ok(error_response());
    }
    match orx.await {
        Ok((status, headers, body)) => {
            let mut builder = hyper::Response::builder().status(status);
            for h in &headers {
                if let [k, v] = h.as_slice() {
                    builder = builder.header(k, v);
                }
            }
            Ok(builder
                .body(Full::new(bytes::Bytes::from(body)))
                .unwrap_or_else(|_| error_response()))
        }
        Err(_) => Ok(error_response()),
    }
}

async fn js_handle(context: AsyncContext, req: ServeRequest) {
    let ServeRequest { port, method, url, headers, body, resp } = req;
    let result = async_with!(context => |ctx| {
        let run = async {
            let f: Function = ctx.globals().get("__handleRequest")?;
            let body = TypedArray::new(ctx.clone(), body)?;
            let p: Promise = f.call((port, method, url, headers, body))?;
            let arr: rquickjs::Array = p.into_future().await?;
            let status: u16 = arr.get(0)?;
            let headers: Vec<Vec<String>> = arr.get(1)?;
            let body: TypedArray<u8> = arr.get(2)?;
            let body = body.as_bytes().map(|b| b.to_vec()).unwrap_or_default();
            Ok::<_, Error>((status, headers, body))
        };
        run.await.catch(&ctx).map_err(|e| e.to_string())
    })
    .await;
    match result {
        Ok(reply) => {
            let _ = resp.send(reply);
        }
        Err(e) => {
            eprintln!("serve error: {}", e);
            let _ = resp.send((500, Vec::new(), b"internal error".to_vec()));
        }
    }
}
