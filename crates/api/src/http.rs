use crate::auth::{Authenticator, Principal};
use crate::config::{AppConfig, CorsConfig};
use crate::graphql::{ApiSchema, build_schema};
use crate::storage::SharedStorage;
use anyhow::Context;
use async_graphql::http::GraphiQLSource;
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::extract::{Request, State};
use axum::http::{HeaderValue, Method, header};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Extension, Router};
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;

#[derive(Clone)]
struct AppState {
    schema: ApiSchema,
    authenticator: Option<Arc<Authenticator>>,
    storage: Option<SharedStorage>,
}

/// Build the schema, wire authentication, bind the configured address and serve.
/// The database is connected only to back the session store; if it is
/// unavailable the server still serves anonymous traffic (e.g. `{ hello }`).
pub async fn serve(config: &AppConfig) -> anyhow::Result<()> {
    let authenticator = build_authenticator(config);
    let storage = crate::storage::build_storage(&config.storage);
    let app = router(build_schema(), authenticator, storage, &config.cors)?;
    let address = format!("{}:{}", config.server.host, config.server.port);
    let listener = TcpListener::bind(&address)
        .await
        .with_context(|| format!("binding {address}"))?;

    tracing::info!(%address, "graphql server listening on POST /graphql");
    axum::serve(listener, app)
        .await
        .context("graphql server error")?;
    Ok(())
}

/// Construct the configured authenticator. The session store uses a lazy pool,
/// so this never blocks on the database: startup is instant and a request that
/// presents no credentials is served without touching Postgres. Returns `None`
/// (anonymous-only) only when the database url is syntactically invalid.
fn build_authenticator(config: &AppConfig) -> Option<Arc<Authenticator>> {
    #[cfg(not(feature = "auth-session"))]
    let _ = config;

    #[cfg(feature = "auth-session")]
    if config.auth.provider == "session" {
        return match crate::db::connect_lazy(&config.database.url) {
            Ok(pool) => {
                let provider = crate::auth::SessionAuthProvider::new(
                    config.auth.session.clone(),
                    crate::auth::SessionStore::new(pool),
                );
                Some(Arc::new(Authenticator::Session(provider)))
            }
            Err(error) => {
                tracing::warn!(%error, "invalid database url; authentication disabled (anonymous only)");
                None
            }
        };
    }

    None
}

/// Assemble the router: `POST /graphql` executes operations, `GET /graphql`
/// serves the GraphiQL IDE. The auth middleware resolves an `Option<Principal>`
/// once per request and exposes it two ways — as request `Extension` (axum
/// handlers) and, injected below, as async-graphql context data (resolvers).
pub fn router(
    schema: ApiSchema,
    authenticator: Option<Arc<Authenticator>>,
    storage: Option<SharedStorage>,
    cors: &CorsConfig,
) -> anyhow::Result<Router> {
    // graphql-ws (native) is served by async-graphql-axum; built from the schema
    // before it moves into the shared state.
    #[cfg(feature = "subs-ws")]
    let ws_service = async_graphql_axum::GraphQLSubscription::new(schema.clone());

    let state = AppState {
        schema,
        authenticator,
        storage,
    };

    #[cfg_attr(not(any(feature = "subs-sse", feature = "subs-ws")), allow(unused_mut))]
    let mut app = Router::new().route("/graphql", get(graphiql).post(graphql_handler));

    // SSE adapter (graphql-sse compatible; the transport the Node SPA can consume
    // unchanged).
    #[cfg(feature = "subs-sse")]
    {
        // GET so the browser's EventSource (which only issues GET) can consume it;
        // POST for programmatic clients. async-graphql's GraphQLRequest reads the
        // operation from the query string on GET.
        app = app.route("/graphql/sse", get(graphql_sse).post(graphql_sse));
    }
    #[cfg(feature = "subs-ws")]
    {
        app = app.route_service("/graphql/ws", ws_service);
    }

    let app = app
        .layer(middleware::from_fn_with_state(state.clone(), authenticate))
        .layer(cors_layer(cors)?)
        .with_state(state);
    Ok(app)
}

/// Resolve the request's principal once and stash it in the request extensions.
async fn authenticate(State(state): State<AppState>, mut request: Request, next: Next) -> Response {
    let principal = match &state.authenticator {
        Some(authenticator) => match authenticator.authenticate(request.headers()).await {
            Ok(principal) => principal,
            Err(error) => {
                tracing::debug!(%error, "authentication failed; treating request as anonymous");
                None
            }
        },
        None => None,
    };
    request
        .extensions_mut()
        .insert::<Option<Principal>>(principal);
    next.run(request).await
}

async fn graphql_handler(
    State(state): State<AppState>,
    Extension(principal): Extension<Option<Principal>>,
    request: GraphQLRequest,
) -> GraphQLResponse {
    let mut request = request.into_inner().data(principal);
    if let Some(storage) = &state.storage {
        request = request.data(storage.clone());
    }
    state.schema.execute(request).await.into()
}

/// SSE adapter: run the operation as a stream and emit each GraphQL response as a
/// `next` server-sent event. This is what lets the Node SPA consume subscriptions
/// over plain HTTP without a WebSocket.
#[cfg(feature = "subs-sse")]
async fn graphql_sse(
    State(state): State<AppState>,
    Extension(principal): Extension<Option<Principal>>,
    request: GraphQLRequest,
) -> axum::response::Sse<
    impl async_graphql::futures_util::Stream<
        Item = Result<axum::response::sse::Event, std::convert::Infallible>,
    >,
> {
    use async_graphql::futures_util::StreamExt;

    let mut request = request.into_inner().data(principal);
    if let Some(storage) = &state.storage {
        request = request.data(storage.clone());
    }

    let events = state.schema.execute_stream(request).map(|response| {
        let json = serde_json::to_string(&response).unwrap_or_default();
        Ok::<_, std::convert::Infallible>(
            axum::response::sse::Event::default()
                .event("next")
                .data(json),
        )
    });

    axum::response::Sse::new(events).keep_alive(axum::response::sse::KeepAlive::default())
}

async fn graphiql() -> impl IntoResponse {
    Html(GraphiQLSource::build().endpoint("/graphql").finish())
}

/// CORS policy driven entirely by `[cors].origins`. Credentials are allowed
/// because auth is cookie/session based, which rules out a wildcard origin.
fn cors_layer(cors: &CorsConfig) -> anyhow::Result<CorsLayer> {
    let origins = cors
        .origins
        .iter()
        .map(|origin| {
            origin
                .parse::<HeaderValue>()
                .with_context(|| format!("invalid CORS origin `{origin}`"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])
        .allow_credentials(true))
}

#[cfg(test)]
mod tests {
    use super::router;
    use crate::build_schema;
    use crate::config::CorsConfig;
    use crate::storage::{SharedStorage, Storage, StorageError};
    use axum::body::{Body, to_bytes};
    use axum::http::{Method, Request, StatusCode, header};
    use std::sync::{Arc, Mutex};
    use tower::ServiceExt;

    fn cors() -> CorsConfig {
        CorsConfig {
            origins: vec!["http://localhost:5173".to_owned()],
        }
    }

    #[derive(Default)]
    struct MockStorage {
        puts: Mutex<Vec<(String, Vec<u8>, String)>>,
    }

    #[async_trait::async_trait]
    impl Storage for MockStorage {
        async fn put(
            &self,
            key: &str,
            bytes: Vec<u8>,
            content_type: &str,
        ) -> Result<String, StorageError> {
            self.puts
                .lock()
                .expect("lock")
                .push((key.to_owned(), bytes, content_type.to_owned()));
            Ok(format!("mock://{key}"))
        }
    }

    #[tokio::test]
    async fn post_graphql_executes_hello_anonymously() {
        let app = router(build_schema(), None, None, &cors()).expect("router builds");
        let request = Request::builder()
            .method(Method::POST)
            .uri("/graphql")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"query":"{ hello }"}"#))
            .expect("request builds");

        let response = app.oneshot(request).await.expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body bytes");
        let text = String::from_utf8(bytes.to_vec()).expect("utf8 body");
        assert!(text.contains("Hello World!"), "unexpected body: {text}");
    }

    #[tokio::test]
    async fn get_graphql_serves_graphiql() {
        let app = router(build_schema(), None, None, &cors()).expect("router builds");
        let request = Request::builder()
            .method(Method::GET)
            .uri("/graphql")
            .body(Body::empty())
            .expect("request builds");

        let response = app.oneshot(request).await.expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body bytes");
        let text = String::from_utf8(bytes.to_vec()).expect("utf8 body");
        assert!(text.contains("GraphiQL"), "expected GraphiQL IDE html");
    }

    #[tokio::test]
    async fn multipart_upload_reaches_storage() {
        let mock = Arc::new(MockStorage::default());
        let storage: SharedStorage = mock.clone();
        let app = router(build_schema(), None, Some(storage), &cors()).expect("router builds");

        // GraphQL multipart request spec (jaydenseric / apollo-upload-client).
        let boundary = "TESTBOUNDARY";
        let body = [
            format!("--{boundary}"),
            "Content-Disposition: form-data; name=\"operations\"".to_owned(),
            String::new(),
            "{\"query\":\"mutation($f: Upload!){ uploadMedia(file: $f) }\",\"variables\":{\"f\":null}}".to_owned(),
            format!("--{boundary}"),
            "Content-Disposition: form-data; name=\"map\"".to_owned(),
            String::new(),
            "{\"0\":[\"variables.f\"]}".to_owned(),
            format!("--{boundary}"),
            "Content-Disposition: form-data; name=\"0\"; filename=\"hello.txt\"".to_owned(),
            "Content-Type: text/plain".to_owned(),
            String::new(),
            "hello bytes".to_owned(),
            format!("--{boundary}--"),
            String::new(),
        ]
        .join("\r\n");

        let request = Request::builder()
            .method(Method::POST)
            .uri("/graphql")
            .header(
                header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(Body::from(body))
            .expect("request builds");

        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body bytes");
        let text = String::from_utf8(bytes.to_vec()).expect("utf8 body");
        assert!(
            text.contains("mock://uploads/hello.txt"),
            "unexpected body: {text}"
        );

        let puts = mock.puts.lock().expect("lock");
        assert_eq!(puts.len(), 1);
        assert_eq!(puts[0].0, "uploads/hello.txt");
        assert_eq!(puts[0].1, b"hello bytes");
        assert_eq!(puts[0].2, "text/plain");
    }

    #[cfg(feature = "subs-sse")]
    #[tokio::test]
    async fn sse_streams_subscription_events() {
        let app = router(build_schema(), None, None, &cors()).expect("router builds");
        let request = Request::builder()
            .method(Method::POST)
            .uri("/graphql/sse")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"query":"subscription { count(to: 3) }"}"#))
            .expect("request builds");

        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        assert!(
            content_type.contains("text/event-stream"),
            "content-type: {content_type}"
        );

        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body bytes");
        let text = String::from_utf8(bytes.to_vec()).expect("utf8 body");
        assert!(text.contains("\"count\":0"), "body: {text}");
        assert!(text.contains("\"count\":1"), "body: {text}");
        assert!(text.contains("\"count\":2"), "body: {text}");
    }
}
