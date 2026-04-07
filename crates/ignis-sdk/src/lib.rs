//! Guest-side Rust SDK for Ignis workers.
//!
//! The crate currently provides:
//! - `http`: lightweight routing, middleware, and response helpers
//! - `sqlite`: guest wrappers around the shared host ABI

/// SQLite bindings and migration helpers exposed to guest workers.
///
/// These APIs forward to the host ABI generated from WIT and are intended for
/// the default SQLite database mounted for the current worker instance.
pub mod sqlite {
    wit_bindgen::generate!({
        path: "../ignis-host-abi/wit",
        world: "imports",
    });

    /// Re-exported low-level SQLite result and statement types generated from
    /// the host ABI.
    pub use ignis::platform::sqlite::{
        QueryResult, Row, SqliteValue, Statement, TypedQueryResult, TypedRow,
    };

    /// Executes a single SQL statement and returns the number of affected rows.
    ///
    /// `params` are converted to owned strings before crossing the guest/host
    /// ABI boundary.
    pub fn execute(sql: &str, params: &[impl AsRef<str>]) -> Result<u64, String> {
        let params = params
            .iter()
            .map(|value| value.as_ref().to_owned())
            .collect::<Vec<_>>();
        ignis::platform::sqlite::execute(sql, &params)
    }

    /// Executes a query and returns rows in the untyped host ABI format.
    ///
    /// Use this when you need direct access to raw SQLite values returned by
    /// the runtime.
    pub fn query(sql: &str, params: &[impl AsRef<str>]) -> Result<QueryResult, String> {
        let params = params
            .iter()
            .map(|value| value.as_ref().to_owned())
            .collect::<Vec<_>>();
        ignis::platform::sqlite::query(sql, &params)
    }

    /// Executes a batch of SQL statements separated by semicolons.
    ///
    /// This is useful for schema setup or other multi-statement initialization
    /// that does not need per-statement parameter binding.
    pub fn execute_batch(sql: &str) -> Result<u64, String> {
        ignis::platform::sqlite::execute_batch(sql)
    }

    /// Executes multiple prepared statements inside a single transaction.
    ///
    /// The host guarantees that either all statements are committed or the
    /// whole transaction is rolled back.
    pub fn transaction(statements: &[Statement]) -> Result<u64, String> {
        ignis::platform::sqlite::transaction(statements)
    }

    /// Executes a query and returns rows in the typed host ABI format.
    ///
    /// This is a better fit than [`query`] when you want explicit SQLite type
    /// information for each returned column.
    pub fn query_typed(sql: &str, params: &[impl AsRef<str>]) -> Result<TypedQueryResult, String> {
        let params = params
            .iter()
            .map(|value| value.as_ref().to_owned())
            .collect::<Vec<_>>();
        ignis::platform::sqlite::query_typed(sql, &params)
    }

    /// Helpers for idempotent schema migrations stored in SQLite itself.
    pub mod migrations {
        use super::{Statement, execute_batch, query, transaction};

        /// Describes a single schema migration.
        ///
        /// Migrations are tracked by `id` inside the `_ignis_migrations` table
        /// and executed in the order they are provided to [`apply`].
        pub struct Migration {
            /// Stable unique identifier for the migration.
            pub id: &'static str,
            /// SQL to execute when the migration has not been applied yet.
            pub sql: &'static str,
        }

        /// Applies any migrations whose `id` has not been recorded yet.
        ///
        /// The function creates `_ignis_migrations` if needed, runs pending
        /// migrations in a single transaction, and returns the list of IDs that
        /// were applied during this call.
        pub fn apply(migrations: &[Migration]) -> Result<Vec<String>, String> {
            execute_batch(
                "create table if not exists _ignis_migrations (
                    id text primary key,
                    applied_at_ms integer not null
                );",
            )?;

            let empty: [&str; 0] = [];
            let existing = query("select id from _ignis_migrations order by id asc", &empty)?;
            let applied = existing
                .rows
                .iter()
                .filter_map(|row| row.values.first().cloned())
                .collect::<std::collections::BTreeSet<_>>();

            let mut applied_now = Vec::new();
            let mut statements = Vec::new();
            for migration in migrations {
                if applied.contains(migration.id) {
                    continue;
                }
                statements.push(Statement {
                    sql: migration.sql.to_owned(),
                    params: Vec::new(),
                });
                statements.push(Statement {
                    sql: "insert into _ignis_migrations (id, applied_at_ms) values (?, strftime('%s','now') * 1000)".to_owned(),
                    params: vec![migration.id.to_owned()],
                });
                applied_now.push(migration.id.to_owned());
            }
            if !statements.is_empty() {
                transaction(&statements)?;
            }
            Ok(applied_now)
        }
    }
}

/// Lightweight HTTP routing, middleware, and response helpers for guest
/// workers.
///
/// The module is intentionally small: a [`crate::http::Router`] stores route
/// handlers, [`crate::http::Context`] exposes the current request plus path
/// parameters, and middleware can wrap handlers through
/// [`crate::http::Middleware`] and [`crate::http::Next`].
pub mod http {
    use std::collections::{BTreeMap, HashMap};
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use http::header::{
        ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderName, HeaderValue,
    };
    use matchit::Router as PathRouter;
    use wstd::http::{Body, Method, Request, Response, StatusCode};

    /// Boxed future used internally to erase handler and middleware future
    /// types.
    type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + 'static>>;
    /// Boxed request handler used by the router dispatch path.
    type BoxHandler = Arc<dyn Fn(Context) -> BoxFuture<Response<Body>> + 'static>;

    /// Handle to the remaining middleware chain plus the final route handler.
    ///
    /// Middleware receives a [`Next`] and can decide whether to call
    /// [`Next::run`] to continue the chain.
    #[derive(Clone)]
    pub struct Next {
        middlewares: Arc<Vec<Middleware>>,
        endpoint: BoxHandler,
        index: usize,
    }

    impl Next {
        /// Runs the next middleware in the chain or the final route handler.
        pub async fn run(self, context: Context) -> Response<Body> {
            if let Some(middleware) = self.middlewares.get(self.index) {
                let next = Self {
                    middlewares: self.middlewares.clone(),
                    endpoint: self.endpoint.clone(),
                    index: self.index + 1,
                };
                middleware.run(context, next).await
            } else {
                (self.endpoint)(context).await
            }
        }
    }

    /// Reusable middleware wrapper constructed by [`middleware()`].
    #[derive(Clone)]
    pub struct Middleware(Arc<dyn Fn(Context, Next) -> BoxFuture<Response<Body>> + 'static>);

    impl Middleware {
        /// Executes the middleware for the provided request context.
        pub async fn run(&self, context: Context, next: Next) -> Response<Body> {
            (self.0)(context, next).await
        }
    }

    /// Wraps an async function into a [`Middleware`] value.
    ///
    /// This is the main entry point for building custom middleware layers.
    pub fn middleware<F, Fut>(func: F) -> Middleware
    where
        F: Fn(Context, Next) -> Fut + 'static,
        Fut: Future<Output = Response<Body>> + 'static,
    {
        Middleware(Arc::new(move |context, next| Box::pin(func(context, next))))
    }

    /// Request context passed to middleware and route handlers.
    ///
    /// It contains the mutable request plus any path parameters extracted by
    /// the router.
    pub struct Context {
        request: Request<Body>,
        params: BTreeMap<String, String>,
    }

    impl Context {
        /// Creates a new context from the incoming request and matched params.
        fn new(request: Request<Body>, params: BTreeMap<String, String>) -> Self {
            Self { request, params }
        }

        /// Returns the HTTP method of the current request.
        pub fn method(&self) -> &Method {
            self.request.method()
        }

        /// Returns the normalized request path.
        pub fn path(&self) -> &str {
            self.request.uri().path()
        }

        /// Returns an immutable reference to the underlying request.
        pub fn request(&self) -> &Request<Body> {
            &self.request
        }

        /// Returns a mutable reference to the underlying request.
        ///
        /// Middleware can use this to attach data in extensions or to mutate
        /// headers before the handler runs.
        pub fn request_mut(&mut self) -> &mut Request<Body> {
            &mut self.request
        }

        /// Consumes the context and returns the owned request.
        pub fn into_request(self) -> Request<Body> {
            self.request
        }

        /// Returns the value of a named path parameter if one was matched.
        pub fn param(&self, name: &str) -> Option<&str> {
            self.params.get(name).map(String::as_str)
        }

        /// Returns all matched path parameters.
        pub fn params(&self) -> &BTreeMap<String, String> {
            &self.params
        }

        /// Returns the request identifier inserted by
        /// [`middleware::request_id`], if present.
        pub fn request_id(&self) -> Option<&str> {
            self.request
                .extensions()
                .get::<RequestId>()
                .map(|request_id| request_id.0.as_str())
        }
    }

    /// Internal route entry storing a resolved handler.
    #[derive(Clone)]
    struct Route {
        handler: BoxHandler,
    }

    /// In-memory HTTP router for guest worker request handling.
    ///
    /// Routes are matched by HTTP method and path. Path segments may use
    /// `:name` syntax for named captures and `*rest` syntax for wildcards.
    #[derive(Default)]
    pub struct Router {
        routes: Vec<Route>,
        by_method: HashMap<String, PathRouter<usize>>,
        middlewares: Vec<Middleware>,
    }

    impl Router {
        /// Creates an empty router.
        pub fn new() -> Self {
            Self::default()
        }

        /// Appends a middleware to the router-wide middleware chain.
        ///
        /// Middleware runs in registration order for every matched request.
        pub fn use_middleware(&mut self, middleware: Middleware) -> &mut Self {
            self.middlewares.push(middleware);
            self
        }

        /// Registers a route handler for the given method and path pattern.
        ///
        /// Returns an error when the pattern cannot be inserted into the
        /// internal matcher.
        pub fn route<F, Fut>(
            &mut self,
            method: Method,
            path: &str,
            handler: F,
        ) -> Result<&mut Self, String>
        where
            F: Fn(Context) -> Fut + 'static,
            Fut: Future<Output = Response<Body>> + 'static,
        {
            let route_index = self.routes.len();
            self.routes.push(Route {
                handler: Arc::new(move |context| Box::pin(handler(context))),
            });
            self.by_method
                .entry(method.as_str().to_owned())
                .or_default()
                .insert(&normalize_route_pattern(path), route_index)
                .map_err(|error| {
                    format!("registering route `{}` {} failed: {error}", method, path)
                })?;
            Ok(self)
        }

        /// Registers a `GET` handler.
        pub fn get<F, Fut>(&mut self, path: &str, handler: F) -> Result<&mut Self, String>
        where
            F: Fn(Context) -> Fut + 'static,
            Fut: Future<Output = Response<Body>> + 'static,
        {
            self.route(Method::GET, path, handler)
        }

        /// Registers a `POST` handler.
        pub fn post<F, Fut>(&mut self, path: &str, handler: F) -> Result<&mut Self, String>
        where
            F: Fn(Context) -> Fut + 'static,
            Fut: Future<Output = Response<Body>> + 'static,
        {
            self.route(Method::POST, path, handler)
        }

        /// Registers a `PUT` handler.
        pub fn put<F, Fut>(&mut self, path: &str, handler: F) -> Result<&mut Self, String>
        where
            F: Fn(Context) -> Fut + 'static,
            Fut: Future<Output = Response<Body>> + 'static,
        {
            self.route(Method::PUT, path, handler)
        }

        /// Registers a `PATCH` handler.
        pub fn patch<F, Fut>(&mut self, path: &str, handler: F) -> Result<&mut Self, String>
        where
            F: Fn(Context) -> Fut + 'static,
            Fut: Future<Output = Response<Body>> + 'static,
        {
            self.route(Method::PATCH, path, handler)
        }

        /// Registers a `DELETE` handler.
        pub fn delete<F, Fut>(&mut self, path: &str, handler: F) -> Result<&mut Self, String>
        where
            F: Fn(Context) -> Fut + 'static,
            Fut: Future<Output = Response<Body>> + 'static,
        {
            self.route(Method::DELETE, path, handler)
        }

        /// Registers an `OPTIONS` handler.
        pub fn options<F, Fut>(&mut self, path: &str, handler: F) -> Result<&mut Self, String>
        where
            F: Fn(Context) -> Fut + 'static,
            Fut: Future<Output = Response<Body>> + 'static,
        {
            self.route(Method::OPTIONS, path, handler)
        }

        /// Dispatches a request through route matching and middleware.
        ///
        /// Requests that match the path but not the method receive `405 Method
        /// Not Allowed`; unmatched paths receive `404 Not Found`.
        pub async fn handle(&self, request: Request<Body>) -> Response<Body> {
            let method = request.method().as_str().to_owned();
            let path = request.uri().path().to_owned();
            let (handler, params) = match self.resolve(&method, &path) {
                Some((handler, params)) => (handler, params),
                None if self.matches_any_method(&path) => {
                    (fallback_handler(method_not_allowed), BTreeMap::new())
                }
                None => (fallback_handler(not_found), BTreeMap::new()),
            };

            let next = Next {
                middlewares: Arc::new(self.middlewares.clone()),
                endpoint: handler,
                index: 0,
            };
            next.run(Context::new(request, params)).await
        }

        /// Resolves a handler and path params for a method/path pair.
        fn resolve(
            &self,
            method: &str,
            path: &str,
        ) -> Option<(BoxHandler, BTreeMap<String, String>)> {
            let router = self.by_method.get(method)?;
            let matched = router.at(path).ok()?;
            let route = self.routes.get(*matched.value)?;
            let params = matched
                .params
                .iter()
                .map(|(name, value)| (name.to_owned(), value.to_owned()))
                .collect::<BTreeMap<_, _>>();
            Some((route.handler.clone(), params))
        }

        /// Returns `true` when any registered method matches the supplied path.
        fn matches_any_method(&self, path: &str) -> bool {
            self.by_method
                .values()
                .any(|router| router.at(path).is_ok())
        }
    }

    /// Built-in middleware helpers.
    pub mod middleware {
        use super::{
            ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS,
            ACCESS_CONTROL_ALLOW_ORIGIN, Context, HeaderName, HeaderValue, Method, Middleware,
            Next, RequestId, StatusCode, empty_response, middleware,
        };

        /// Adds a generated request ID to the request extensions and response
        /// headers.
        ///
        /// The ID is exposed to handlers through [`Context::request_id`] and is
        /// returned to clients as `x-request-id`.
        pub fn request_id() -> Middleware {
            middleware(|mut context: Context, next: Next| async move {
                let request_id = RequestId::new();
                context
                    .request_mut()
                    .extensions_mut()
                    .insert(request_id.clone());
                let mut response = next.run(context).await;
                response.headers_mut().insert(
                    HeaderName::from_static("x-request-id"),
                    HeaderValue::from_str(&request_id.0)
                        .unwrap_or_else(|_| HeaderValue::from_static("invalid-request-id")),
                );
                response
            })
        }

        /// Logs one line per request to standard output.
        ///
        /// The log includes method, path, status, duration, and request ID when
        /// available.
        pub fn logger() -> Middleware {
            middleware(|context: Context, next: Next| async move {
                let method = context.method().as_str().to_owned();
                let path = context.path().to_owned();
                let request_id = context.request_id().map(str::to_owned);
                let started = std::time::Instant::now();
                let response = next.run(context).await;
                println!(
                    "[ignis] method={} path={} status={} duration_ms={} request_id={}",
                    method,
                    path,
                    response.status().as_u16(),
                    started.elapsed().as_millis(),
                    request_id.as_deref().unwrap_or("-"),
                );
                response
            })
        }

        /// Applies permissive CORS headers and short-circuits `OPTIONS`
        /// preflight requests.
        pub fn cors() -> Middleware {
            middleware(|context: Context, next: Next| async move {
                if context.method() == Method::OPTIONS {
                    let mut response = empty_response(StatusCode::NO_CONTENT);
                    apply_cors_headers(&mut response);
                    return response;
                }
                let mut response = next.run(context).await;
                apply_cors_headers(&mut response);
                response
            })
        }

        /// Inserts the default CORS response headers used by [`cors`].
        fn apply_cors_headers(response: &mut wstd::http::Response<wstd::http::Body>) {
            response
                .headers_mut()
                .insert(ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
            response.headers_mut().insert(
                ACCESS_CONTROL_ALLOW_HEADERS,
                HeaderValue::from_static("content-type, authorization, x-request-id"),
            );
            response.headers_mut().insert(
                ACCESS_CONTROL_ALLOW_METHODS,
                HeaderValue::from_static("GET, POST, PUT, PATCH, DELETE, OPTIONS"),
            );
        }
    }

    /// Internal request identifier stored in request extensions.
    #[derive(Clone)]
    struct RequestId(String);

    impl RequestId {
        /// Generates a best-effort unique request identifier for the current
        /// process.
        fn new() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(1);
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_millis() as u64)
                .unwrap_or(0);
            let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
            Self(format!("req-{now_ms}-{sequence}"))
        }
    }

    /// Builds the default `404 Not Found` response.
    fn not_found() -> Response<Body> {
        text_response(StatusCode::NOT_FOUND, "route not found\n")
    }

    /// Builds the default `405 Method Not Allowed` response.
    fn method_not_allowed() -> Response<Body> {
        text_response(StatusCode::METHOD_NOT_ALLOWED, "method not allowed\n")
    }

    /// Normalizes Ignis route syntax to the pattern format expected by
    /// `matchit`.
    ///
    /// `:name` is converted to `{name}` and `*rest` is converted to `{*rest}`.
    fn normalize_route_pattern(path: &str) -> String {
        let mut normalized = String::with_capacity(path.len());
        for segment in path.split('/') {
            if normalized.is_empty() {
                normalized.push('/');
            } else if !normalized.ends_with('/') {
                normalized.push('/');
            }
            if let Some(name) = segment.strip_prefix(':') {
                normalized.push('{');
                normalized.push_str(name);
                normalized.push('}');
            } else if let Some(name) = segment.strip_prefix('*') {
                normalized.push('{');
                normalized.push('*');
                normalized.push_str(name);
                normalized.push('}');
            } else {
                normalized.push_str(segment);
            }
        }
        normalized
    }

    /// Wraps a fixed response builder into a boxed handler.
    fn fallback_handler(response: fn() -> Response<Body>) -> BoxHandler {
        Arc::new(move |_| Box::pin(async move { response() }))
    }

    /// Creates a plain-text response with the given HTTP status.
    pub fn text_response(status: StatusCode, body: impl Into<String>) -> Response<Body> {
        Response::builder()
            .status(status)
            .body(body.into().into())
            .expect("response build")
    }

    /// Creates an empty response with the given HTTP status.
    pub fn empty_response(status: StatusCode) -> Response<Body> {
        Response::builder()
            .status(status)
            .body(String::new().into())
            .expect("response build")
    }

    #[cfg(test)]
    mod tests {
        use super::{Context, Router, middleware};
        use http::HeaderValue;
        use wstd::http::{Method, Request, StatusCode};

        #[tokio::test]
        async fn matches_route_params() {
            let mut router = Router::new();
            router
                .get("/tasks/:id", |context: Context| async move {
                    super::text_response(
                        StatusCode::OK,
                        context.param("id").unwrap_or("missing").to_owned(),
                    )
                })
                .unwrap();

            let request = Request::builder()
                .method(Method::GET)
                .uri("http://localhost/tasks/abc")
                .body(String::new().into())
                .unwrap();
            let response = router.handle(request).await;
            assert_eq!(response.status(), StatusCode::OK);
        }

        #[tokio::test]
        async fn runs_middleware_chain() {
            let mut router = Router::new();
            router.use_middleware(middleware(|mut context: Context, next| async move {
                context
                    .request_mut()
                    .headers_mut()
                    .insert("x-chain", HeaderValue::from_static("seen"));
                let mut response = next.run(context).await;
                response
                    .headers_mut()
                    .insert("x-chain", HeaderValue::from_static("done"));
                response
            }));
            router
                .get("/", |context: Context| async move {
                    assert_eq!(
                        context
                            .request()
                            .headers()
                            .get("x-chain")
                            .and_then(|value| value.to_str().ok()),
                        Some("seen")
                    );
                    super::empty_response(StatusCode::NO_CONTENT)
                })
                .unwrap();

            let request = Request::builder()
                .method(Method::GET)
                .uri("http://localhost/")
                .body(String::new().into())
                .unwrap();
            let response = router.handle(request).await;
            assert_eq!(response.status(), StatusCode::NO_CONTENT);
            assert_eq!(
                response
                    .headers()
                    .get("x-chain")
                    .and_then(|value| value.to_str().ok()),
                Some("done")
            );
        }
    }
}
