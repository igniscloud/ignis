//! Runtime execution core for Ignis workers.
//!
//! This crate focuses on:
//! - Wasmtime engine and component loading
//! - WASI and `wasi:http` integration
//! - request dispatch
//! - store limits and CPU interruption
//! - outbound HTTP transport hooks

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use http::{Response, StatusCode, Uri};
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use ignis_manifest::LoadedManifest;
use ignis_platform_host::{HostBindings, SqliteHost};
use tokio::net::TcpListener;
use tracing::{error, info};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store, StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};
use wasmtime_wasi_http::WasiHttpCtx;
use wasmtime_wasi_http::io::TokioIo;
use wasmtime_wasi_http::p2::bindings::ProxyPre;
use wasmtime_wasi_http::p2::bindings::http::types::Scheme;
use wasmtime_wasi_http::p2::body::HyperOutgoingBody;
use wasmtime_wasi_http::p2::{
    HttpResult, WasiHttpCtxView, WasiHttpHooks, WasiHttpView, default_send_request, types,
};

#[derive(Debug, Clone)]
pub struct DevServerConfig {
    pub listen_addr: SocketAddr,
}

#[derive(Debug, Clone)]
pub struct WorkerRuntimeOptions {
    pub epoch_tick_interval: Duration,
    pub internal_http_dispatch: Option<InternalHttpDispatchConfig>,
}

#[derive(Debug, Clone)]
pub struct InternalHttpDispatchConfig {
    pub base_url: String,
    pub bearer_token: String,
    pub caller_project: Option<String>,
}

impl Default for WorkerRuntimeOptions {
    fn default() -> Self {
        Self {
            epoch_tick_interval: Duration::from_millis(10),
            internal_http_dispatch: None,
        }
    }
}

pub async fn serve(manifest: LoadedManifest, config: DevServerConfig) -> Result<()> {
    let runtime = Arc::new(WorkerRuntime::<SqliteHost>::load(manifest)?);
    let listener = TcpListener::bind(config.listen_addr)
        .await
        .with_context(|| format!("binding dev server on {}", config.listen_addr))?;

    info!(
        listen_addr = %listener.local_addr()?,
        app = %runtime.manifest.manifest.name,
        component = %runtime.component_path.display(),
        "ignis dev server started",
    );

    loop {
        let (stream, peer_addr) = listener
            .accept()
            .await
            .context("accepting HTTP connection")?;
        let runtime = runtime.clone();
        tokio::spawn(async move {
            let service = service_fn(move |request| {
                let runtime = runtime.clone();
                async move { runtime.handle_request(request).await }
            });

            if let Err(err) = http1::Builder::new()
                .keep_alive(true)
                .serve_connection(TokioIo::new(stream), service)
                .await
            {
                error!(%peer_addr, error = %err, "serving connection failed");
            }
        });
    }
}

#[derive(Clone)]
pub struct WorkerRuntime<H: HostBindings = SqliteHost> {
    engine: Engine,
    pre: ProxyPre<StoreState<H>>,
    manifest: LoadedManifest,
    component_path: Arc<std::path::PathBuf>,
    options: WorkerRuntimeOptions,
}

impl<H: HostBindings> WorkerRuntime<H> {
    pub fn load(manifest: LoadedManifest) -> Result<Self> {
        Self::load_with_options(manifest, WorkerRuntimeOptions::default())
    }

    pub fn load_with_options(
        manifest: LoadedManifest,
        options: WorkerRuntimeOptions,
    ) -> Result<Self> {
        let component_path = manifest.component_path();
        if !component_path.exists() {
            bail!(
                "component wasm not found at {}; run `ignis build` first",
                component_path.display()
            );
        }

        let engine = wasmtime_result(Engine::new(&engine_config()?), "creating wasmtime engine")?;
        let component = wasmtime_result(
            Component::from_file(&engine, &component_path),
            format!("loading component from {}", component_path.display()),
        )?;

        let mut linker = Linker::<StoreState<H>>::new(&engine);
        wasmtime_result(
            wasmtime_wasi::p2::add_to_linker_async(&mut linker),
            "linking WASI p2",
        )?;
        wasmtime_result(
            wasmtime_wasi_http::p2::add_only_http_to_linker_async(&mut linker),
            "linking wasi:http",
        )?;
        wasmtime_result(
            H::add_to_linker(&mut linker, store_host::<H>),
            "linking host imports",
        )?;
        let instance_pre = wasmtime_result(
            linker.instantiate_pre(&component),
            "preparing component instance",
        )?;
        let pre = wasmtime_result(
            ProxyPre::new(instance_pre),
            "pre-instantiating wasi:http component",
        )?;

        Ok(Self {
            engine,
            pre,
            component_path: Arc::new(component_path),
            manifest,
            options,
        })
    }

    pub async fn handle_request(
        self: Arc<Self>,
        request: hyper::Request<Incoming>,
    ) -> std::result::Result<Response<HyperOutgoingBody>, hyper::Error> {
        match self.dispatch(request).await {
            Ok(response) => Ok(response),
            Err(err) => {
                error!(error = %err, "component execution failed");
                Ok(internal_error_response(err))
            }
        }
    }

    pub async fn warm(&self) -> Result<()> {
        let state = StoreState::new(&self.manifest, self.options.clone())?;
        let mut store = Store::new(&self.engine, state);
        configure_store_limits(&mut store);
        configure_cpu_deadline(
            &mut store,
            self.manifest.manifest.resources.cpu_time_limit_ms,
            self.options.epoch_tick_interval,
        );
        let ticker = start_epoch_ticker(
            self.engine.clone(),
            self.manifest.manifest.resources.cpu_time_limit_ms,
            self.options.epoch_tick_interval,
        );
        let result = self
            .pre
            .instantiate_async(&mut store)
            .await
            .map_err(|error| anyhow!(error.context("warming component instance")));
        stop_epoch_ticker(ticker).await;
        result.map(|_| ())
    }

    async fn dispatch(
        &self,
        request: hyper::Request<Incoming>,
    ) -> Result<Response<HyperOutgoingBody>> {
        let request = rewrite_base_path(request, &self.manifest.manifest.base_path)?;

        let state = StoreState::new(&self.manifest, self.options.clone())?;
        let mut store = Store::new(&self.engine, state);
        configure_store_limits(&mut store);
        configure_cpu_deadline(
            &mut store,
            self.manifest.manifest.resources.cpu_time_limit_ms,
            self.options.epoch_tick_interval,
        );
        let ticker = start_epoch_ticker(
            self.engine.clone(),
            self.manifest.manifest.resources.cpu_time_limit_ms,
            self.options.epoch_tick_interval,
        );

        let (sender, receiver) = tokio::sync::oneshot::channel();
        let req = store
            .data_mut()
            .http()
            .new_incoming_request(Scheme::Http, request)
            .map_err(|error| anyhow!(error.context("creating wasi:http incoming request")))?;
        let out = store
            .data_mut()
            .http()
            .new_response_outparam(sender)
            .map_err(|error| anyhow!(error.context("creating response outparam")))?;

        let result = async {
            let proxy = self
                .pre
                .instantiate_async(&mut store)
                .await
                .map_err(|error| anyhow!(error.context("instantiating component")))?;

            proxy
                .wasi_http_incoming_handler()
                .call_handle(&mut store, req, out)
                .await
                .map_err(|error| anyhow!(error.context("calling wasi:http incoming handler")))?;

            match receiver.await {
                Ok(Ok(response)) => Ok(response),
                Ok(Err(err)) => Err(anyhow!("guest returned wasi:http error: {err:?}")),
                Err(_) => Err(anyhow!(
                    "guest returned without setting `response-outparam`; ensure the component exports `wasi:http/incoming-handler`"
                )),
            }
        }
        .await;
        stop_epoch_ticker(ticker).await;
        result
    }

    pub fn manifest(&self) -> &LoadedManifest {
        &self.manifest
    }
}

struct StoreState<H: HostBindings> {
    table: ResourceTable,
    wasi: WasiCtx,
    http: WasiHttpCtx,
    host: H,
    http_hooks: OutboundHttpHooks,
    limits: StoreLimits,
}

impl<H: HostBindings> StoreState<H> {
    fn new(manifest: &LoadedManifest, options: WorkerRuntimeOptions) -> Result<Self> {
        let mut builder = WasiCtxBuilder::new();
        builder.inherit_stdout().inherit_stderr();
        for (key, value) in &manifest.manifest.env {
            builder.env(key, value);
        }
        Ok(Self {
            table: ResourceTable::new(),
            wasi: builder.build(),
            http: WasiHttpCtx::new(),
            host: H::from_manifest(manifest)?,
            http_hooks: OutboundHttpHooks {
                internal_dispatch: options.internal_http_dispatch,
            },
            limits: build_store_limits(manifest)?,
        })
    }

    fn host_mut(&mut self) -> &mut H {
        &mut self.host
    }
}

fn store_host<H: HostBindings>(state: &mut StoreState<H>) -> &mut H {
    state.host_mut()
}

impl<H: HostBindings> WasiView for StoreState<H> {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

impl<H: HostBindings> WasiHttpView for StoreState<H> {
    fn http(&mut self) -> WasiHttpCtxView<'_> {
        WasiHttpCtxView {
            ctx: &mut self.http,
            table: &mut self.table,
            hooks: &mut self.http_hooks,
        }
    }
}

fn engine_config() -> Result<Config> {
    let mut config = Config::new();
    config.wasm_component_model(true);
    config.cranelift_opt_level(wasmtime::OptLevel::Speed);
    config.epoch_interruption(true);
    config.allocation_strategy(wasmtime::InstanceAllocationStrategy::Pooling(
        wasmtime::PoolingAllocationConfig::default(),
    ));
    Ok(config)
}

const INTERNAL_ISL_DISPATCH_PREFIX: &str = "/__ignis_internal/isl/http-dispatch";
const INTERNAL_SERVICE_IDENTITY_HEADER: &str = "x-ignis-service-identity";
const INTERNAL_CALLER_PROJECT_HEADER: &str = "x-ignis-caller-project";

struct OutboundHttpHooks {
    internal_dispatch: Option<InternalHttpDispatchConfig>,
}

impl WasiHttpHooks for OutboundHttpHooks {
    fn send_request(
        &mut self,
        mut request: hyper::Request<HyperOutgoingBody>,
        config: types::OutgoingRequestConfig,
    ) -> HttpResult<types::HostFutureIncomingResponse> {
        if let Some(dispatch) = &self.internal_dispatch {
            if let Some(service_identity) = internal_service_identity(request.uri(), dispatch)? {
                if let Ok(rewritten_uri) =
                    rewrite_internal_dispatch_uri(&dispatch.base_url, request.uri())
                {
                    *request.uri_mut() = rewritten_uri;
                    if let Ok(value) = http::HeaderValue::from_str(&service_identity) {
                        request
                            .headers_mut()
                            .insert(INTERNAL_SERVICE_IDENTITY_HEADER, value);
                    }
                    if let Some(project) = dispatch.caller_project.as_deref() {
                        if let Ok(value) = http::HeaderValue::from_str(project) {
                            request
                                .headers_mut()
                                .insert(INTERNAL_CALLER_PROJECT_HEADER, value);
                        }
                    }
                    if let Ok(value) =
                        http::HeaderValue::from_str(&format!("Bearer {}", dispatch.bearer_token))
                    {
                        request
                            .headers_mut()
                            .insert(http::header::AUTHORIZATION, value);
                    }
                }
            }
        }
        Ok(default_send_request(request, config))
    }
}

fn build_store_limits(manifest: &LoadedManifest) -> Result<StoreLimits> {
    let mut builder = StoreLimitsBuilder::new();
    if let Some(limit) = manifest.manifest.resources.memory_limit_bytes {
        builder = builder
            .memory_size(usize::try_from(limit).context("memory_limit_bytes exceeds host usize")?);
    }
    Ok(builder.build())
}

fn configure_store_limits<H: HostBindings>(store: &mut Store<StoreState<H>>) {
    store.limiter(|state| &mut state.limits);
}

fn configure_cpu_deadline<H: HostBindings>(
    store: &mut Store<StoreState<H>>,
    cpu_time_limit_ms: Option<u64>,
    epoch_tick_interval: Duration,
) {
    #[cfg(target_has_atomic = "64")]
    {
        store.epoch_deadline_trap();
        store.set_epoch_deadline(deadline_ticks(cpu_time_limit_ms, epoch_tick_interval));
    }
}

fn deadline_ticks(cpu_time_limit_ms: Option<u64>, epoch_tick_interval: Duration) -> u64 {
    let tick_ms = epoch_tick_interval.as_millis().max(1) as u64;
    match cpu_time_limit_ms {
        Some(limit_ms) => limit_ms.saturating_add(tick_ms - 1) / tick_ms,
        None => u64::MAX / 4,
    }
    .max(1)
}

struct EpochTicker {
    stop: Arc<AtomicBool>,
    handle: tokio::task::JoinHandle<()>,
}

fn start_epoch_ticker(
    engine: Engine,
    cpu_time_limit_ms: Option<u64>,
    epoch_tick_interval: Duration,
) -> Option<EpochTicker> {
    if cpu_time_limit_ms.is_none() {
        return None;
    }
    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = stop.clone();
    let handle = tokio::spawn(async move {
        while !stop_flag.load(Ordering::Relaxed) {
            tokio::time::sleep(epoch_tick_interval).await;
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }
            engine.increment_epoch();
        }
    });
    Some(EpochTicker { stop, handle })
}

async fn stop_epoch_ticker(ticker: Option<EpochTicker>) {
    if let Some(ticker) = ticker {
        ticker.stop.store(true, Ordering::Relaxed);
        let _ = ticker.handle.await;
    }
}

fn rewrite_base_path<B>(request: hyper::Request<B>, base_path: &str) -> Result<hyper::Request<B>> {
    if base_path == "/" {
        return Ok(request);
    }

    let path = request.uri().path();
    let prefix = base_path.trim_end_matches('/');
    if !path.starts_with(prefix) {
        return Err(anyhow!(
            "request path `{}` does not match base_path `{}`",
            path,
            base_path
        ));
    }

    let stripped = path
        .strip_prefix(prefix)
        .filter(|candidate| !candidate.is_empty())
        .unwrap_or("/");
    let rebuilt = rebuild_uri(request.uri(), stripped)?;
    let (mut parts, body) = request.into_parts();
    parts.uri = rebuilt;
    Ok(hyper::Request::from_parts(parts, body))
}

fn rebuild_uri(uri: &Uri, path: &str) -> Result<Uri> {
    let mut builder = Uri::builder();
    if let Some(scheme) = uri.scheme_str() {
        builder = builder.scheme(scheme);
    }
    if let Some(authority) = uri.authority() {
        builder = builder.authority(authority.as_str());
    }
    let path_and_query = if let Some(query) = uri.query() {
        format!("{path}?{query}")
    } else {
        path.to_owned()
    };
    builder
        .path_and_query(path_and_query)
        .build()
        .context("rebuilding request URI")
}

fn internal_service_identity(
    uri: &Uri,
    dispatch: &InternalHttpDispatchConfig,
) -> HttpResult<Option<String>> {
    let Some(authority) = uri.authority().map(|authority| authority.host()) else {
        return Ok(None);
    };
    let suffix = ".svc";
    let Some(prefix) = authority.strip_suffix(suffix) else {
        return Ok(None);
    };
    let Some(caller_project) = dispatch.caller_project.as_deref() else {
        return Ok(None);
    };
    let (service, project) = match prefix.rsplit_once('.') {
        Some((service, project)) if project == caller_project => (service, project),
        Some((_, project)) => {
            return Err(wasmtime_wasi_http::p2::bindings::http::types::ErrorCode::InternalError(
                Some(format!(
                    "cross-project service access denied: caller `{caller_project}` cannot access `{project}`"
                )),
            )
            .into());
        }
        None => (prefix, caller_project),
    };
    if service.is_empty() {
        return Ok(None);
    }
    Ok(Some(format!("svc://{project}/{service}#http")))
}

fn rewrite_internal_dispatch_uri(base_url: &str, original_uri: &Uri) -> Result<Uri> {
    let base: Uri = base_url
        .parse()
        .with_context(|| format!("parsing internal dispatch base URL `{base_url}`"))?;
    let path = original_uri.path();
    let dispatch_path = if path == "/" {
        INTERNAL_ISL_DISPATCH_PREFIX.to_owned()
    } else {
        format!("{INTERNAL_ISL_DISPATCH_PREFIX}{path}")
    };
    let mut builder = Uri::builder();
    if let Some(scheme) = base.scheme_str() {
        builder = builder.scheme(scheme);
    }
    if let Some(authority) = base.authority() {
        builder = builder.authority(authority.as_str());
    }
    let path_and_query = if let Some(query) = original_uri.query() {
        format!("{dispatch_path}?{query}")
    } else {
        dispatch_path
    };
    builder
        .path_and_query(path_and_query)
        .build()
        .context("building internal dispatch URI")
}

fn internal_error_response(error: anyhow::Error) -> Response<HyperOutgoingBody> {
    let body = Full::new(hyper::body::Bytes::from(format!(
        "worker execution failed: {error}\n"
    )))
    .map_err(|never| match never {})
    .boxed_unsync();
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(body)
        .expect("internal error response should build")
}

fn wasmtime_result<T, C>(result: wasmtime::Result<T>, context: C) -> Result<T>
where
    C: std::fmt::Display + Send + Sync + 'static,
{
    result.map_err(|error| anyhow!(error.context(context)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::Empty;

    #[test]
    fn strips_base_path() {
        let request = hyper::Request::builder()
            .uri("http://localhost/app/hello?name=wasm")
            .body(Empty::<hyper::body::Bytes>::new())
            .unwrap();

        let request = rewrite_base_path(request, "/app").unwrap();
        assert_eq!(request.uri().path(), "/hello");
        assert_eq!(request.uri().query(), Some("name=wasm"));
    }

    #[test]
    fn derives_internal_service_identity_from_svc_authority() {
        let uri: Uri = "http://api.svc/users?id=1".parse().unwrap();
        let config = InternalHttpDispatchConfig {
            base_url: "http://127.0.0.1:4031".to_owned(),
            bearer_token: "token".to_owned(),
            caller_project: Some("demo-project".to_owned()),
        };

        let identity = internal_service_identity(&uri, &config).unwrap();

        assert_eq!(identity.as_deref(), Some("svc://demo-project/api#http"));
    }

    #[test]
    fn rewrites_internal_dispatch_uri_to_local_node_agent() {
        let original: Uri = "http://api.svc/users?id=1".parse().unwrap();

        let rewritten = rewrite_internal_dispatch_uri("http://127.0.0.1:4031", &original).unwrap();

        assert_eq!(
            rewritten.to_string(),
            "http://127.0.0.1:4031/__ignis_internal/isl/http-dispatch/users?id=1"
        );
    }

    #[test]
    fn accepts_fully_qualified_svc_authority_for_same_project() {
        let uri: Uri = "http://api.demo-project.svc/users?id=1".parse().unwrap();
        let config = InternalHttpDispatchConfig {
            base_url: "http://127.0.0.1:4031".to_owned(),
            bearer_token: "token".to_owned(),
            caller_project: Some("demo-project".to_owned()),
        };

        let identity = internal_service_identity(&uri, &config).unwrap();

        assert_eq!(identity.as_deref(), Some("svc://demo-project/api#http"));
    }

    #[test]
    fn rejects_cross_project_svc_authority() {
        let uri: Uri = "http://api.other-project.svc/users?id=1".parse().unwrap();
        let config = InternalHttpDispatchConfig {
            base_url: "http://127.0.0.1:4031".to_owned(),
            bearer_token: "token".to_owned(),
            caller_project: Some("demo-project".to_owned()),
        };

        let error = internal_service_identity(&uri, &config).unwrap_err();
        let code = error.downcast().unwrap();

        match code {
            wasmtime_wasi_http::p2::bindings::http::types::ErrorCode::InternalError(Some(
                message,
            )) => {
                assert_eq!(
                    message,
                    "cross-project service access denied: caller `demo-project` cannot access `other-project`"
                );
            }
            other => panic!("unexpected error code: {other:?}"),
        }
    }
}
