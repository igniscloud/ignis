//! Guest-side Rust SDK for Ember workers.
//!
//! The crate currently provides:
//! - `http`: lightweight routing, middleware, and response helpers
//! - `sqlite`: guest wrappers around the shared host ABI

pub mod sqlite {
    wit_bindgen::generate!({
        path: "../ember-host-abi/wit",
        world: "imports",
    });

    pub use wkr::platform::sqlite::{
        QueryResult, Row, SqliteValue, Statement, TypedQueryResult, TypedRow,
    };

    pub fn execute(sql: &str, params: &[impl AsRef<str>]) -> Result<u64, String> {
        let params = params
            .iter()
            .map(|value| value.as_ref().to_owned())
            .collect::<Vec<_>>();
        wkr::platform::sqlite::execute(sql, &params)
    }

    pub fn query(sql: &str, params: &[impl AsRef<str>]) -> Result<QueryResult, String> {
        let params = params
            .iter()
            .map(|value| value.as_ref().to_owned())
            .collect::<Vec<_>>();
        wkr::platform::sqlite::query(sql, &params)
    }

    pub fn execute_batch(sql: &str) -> Result<u64, String> {
        wkr::platform::sqlite::execute_batch(sql)
    }

    pub fn transaction(statements: &[Statement]) -> Result<u64, String> {
        wkr::platform::sqlite::transaction(statements)
    }

    pub fn query_typed(sql: &str, params: &[impl AsRef<str>]) -> Result<TypedQueryResult, String> {
        let params = params
            .iter()
            .map(|value| value.as_ref().to_owned())
            .collect::<Vec<_>>();
        wkr::platform::sqlite::query_typed(sql, &params)
    }

    pub mod migrations {
        use super::{Statement, execute_batch, query, transaction};

        pub struct Migration {
            pub id: &'static str,
            pub sql: &'static str,
        }

        pub fn apply(migrations: &[Migration]) -> Result<Vec<String>, String> {
            execute_batch(
                "create table if not exists _ember_migrations (
                    id text primary key,
                    applied_at_ms integer not null
                );",
            )?;

            let empty: [&str; 0] = [];
            let existing = query("select id from _ember_migrations order by id asc", &empty)?;
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
                    sql: "insert into _ember_migrations (id, applied_at_ms) values (?, strftime('%s','now') * 1000)".to_owned(),
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

    type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + 'static>>;
    type BoxHandler = Arc<dyn Fn(Context) -> BoxFuture<Response<Body>> + 'static>;

    #[derive(Clone)]
    pub struct Next {
        middlewares: Arc<Vec<Middleware>>,
        endpoint: BoxHandler,
        index: usize,
    }

    impl Next {
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

    #[derive(Clone)]
    pub struct Middleware(Arc<dyn Fn(Context, Next) -> BoxFuture<Response<Body>> + 'static>);

    impl Middleware {
        pub async fn run(&self, context: Context, next: Next) -> Response<Body> {
            (self.0)(context, next).await
        }
    }

    pub fn middleware<F, Fut>(func: F) -> Middleware
    where
        F: Fn(Context, Next) -> Fut + 'static,
        Fut: Future<Output = Response<Body>> + 'static,
    {
        Middleware(Arc::new(move |context, next| Box::pin(func(context, next))))
    }

    pub struct Context {
        request: Request<Body>,
        params: BTreeMap<String, String>,
    }

    impl Context {
        fn new(request: Request<Body>, params: BTreeMap<String, String>) -> Self {
            Self { request, params }
        }

        pub fn method(&self) -> &Method {
            self.request.method()
        }

        pub fn path(&self) -> &str {
            self.request.uri().path()
        }

        pub fn request(&self) -> &Request<Body> {
            &self.request
        }

        pub fn request_mut(&mut self) -> &mut Request<Body> {
            &mut self.request
        }

        pub fn into_request(self) -> Request<Body> {
            self.request
        }

        pub fn param(&self, name: &str) -> Option<&str> {
            self.params.get(name).map(String::as_str)
        }

        pub fn params(&self) -> &BTreeMap<String, String> {
            &self.params
        }

        pub fn request_id(&self) -> Option<&str> {
            self.request
                .extensions()
                .get::<RequestId>()
                .map(|request_id| request_id.0.as_str())
        }
    }

    #[derive(Clone)]
    struct Route {
        handler: BoxHandler,
    }

    #[derive(Default)]
    pub struct Router {
        routes: Vec<Route>,
        by_method: HashMap<String, PathRouter<usize>>,
        middlewares: Vec<Middleware>,
    }

    impl Router {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn use_middleware(&mut self, middleware: Middleware) -> &mut Self {
            self.middlewares.push(middleware);
            self
        }

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

        pub fn get<F, Fut>(&mut self, path: &str, handler: F) -> Result<&mut Self, String>
        where
            F: Fn(Context) -> Fut + 'static,
            Fut: Future<Output = Response<Body>> + 'static,
        {
            self.route(Method::GET, path, handler)
        }

        pub fn post<F, Fut>(&mut self, path: &str, handler: F) -> Result<&mut Self, String>
        where
            F: Fn(Context) -> Fut + 'static,
            Fut: Future<Output = Response<Body>> + 'static,
        {
            self.route(Method::POST, path, handler)
        }

        pub fn put<F, Fut>(&mut self, path: &str, handler: F) -> Result<&mut Self, String>
        where
            F: Fn(Context) -> Fut + 'static,
            Fut: Future<Output = Response<Body>> + 'static,
        {
            self.route(Method::PUT, path, handler)
        }

        pub fn patch<F, Fut>(&mut self, path: &str, handler: F) -> Result<&mut Self, String>
        where
            F: Fn(Context) -> Fut + 'static,
            Fut: Future<Output = Response<Body>> + 'static,
        {
            self.route(Method::PATCH, path, handler)
        }

        pub fn delete<F, Fut>(&mut self, path: &str, handler: F) -> Result<&mut Self, String>
        where
            F: Fn(Context) -> Fut + 'static,
            Fut: Future<Output = Response<Body>> + 'static,
        {
            self.route(Method::DELETE, path, handler)
        }

        pub fn options<F, Fut>(&mut self, path: &str, handler: F) -> Result<&mut Self, String>
        where
            F: Fn(Context) -> Fut + 'static,
            Fut: Future<Output = Response<Body>> + 'static,
        {
            self.route(Method::OPTIONS, path, handler)
        }

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

        fn matches_any_method(&self, path: &str) -> bool {
            self.by_method
                .values()
                .any(|router| router.at(path).is_ok())
        }
    }

    pub mod middleware {
        use super::{
            ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS,
            ACCESS_CONTROL_ALLOW_ORIGIN, Context, HeaderName, HeaderValue, Method, Middleware,
            Next, RequestId, StatusCode, empty_response, middleware,
        };

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

        pub fn logger() -> Middleware {
            middleware(|context: Context, next: Next| async move {
                let method = context.method().as_str().to_owned();
                let path = context.path().to_owned();
                let request_id = context.request_id().map(str::to_owned);
                let started = std::time::Instant::now();
                let response = next.run(context).await;
                println!(
                    "[wkr] method={} path={} status={} duration_ms={} request_id={}",
                    method,
                    path,
                    response.status().as_u16(),
                    started.elapsed().as_millis(),
                    request_id.as_deref().unwrap_or("-"),
                );
                response
            })
        }

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

    #[derive(Clone)]
    struct RequestId(String);

    impl RequestId {
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

    fn not_found() -> Response<Body> {
        text_response(StatusCode::NOT_FOUND, "route not found\n")
    }

    fn method_not_allowed() -> Response<Body> {
        text_response(StatusCode::METHOD_NOT_ALLOWED, "method not allowed\n")
    }

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

    fn fallback_handler(response: fn() -> Response<Body>) -> BoxHandler {
        Arc::new(move |_| Box::pin(async move { response() }))
    }

    pub fn text_response(status: StatusCode, body: impl Into<String>) -> Response<Body> {
        Response::builder()
            .status(status)
            .body(body.into().into())
            .expect("response build")
    }

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
