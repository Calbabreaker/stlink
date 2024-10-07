use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use shuttle_runtime::tokio;
use tokio::sync::RwLock;

const LINK_LENGTH: usize = 5;
const MAX_LINKS: usize = 10000;
const EXPIRE_DURATION: Duration = Duration::from_secs(5 * 60);
const CLEANUP_INTERVAL: Duration = Duration::from_secs(30);

struct Link {
    data: String,
    expire_time: Instant,
}

type LinkStore = Arc<RwLock<HashMap<String, Link>>>;

#[shuttle_runtime::main]
async fn axum() -> Result<shuttle_axum::AxumService, shuttle_runtime::Error> {
    let store = LinkStore::default();

    cleanup_task(store.clone());

    let api_router = Router::new()
        .route("/", post(create_link))
        .route("/:id", get(get_data_view).delete(delete_link));

    let router = Router::new()
        .route("/script.js", static_route!("script", "js"))
        .route("/style.css", static_route!("style", "css"))
        .route("/", static_route!("index", "html"))
        .merge(api_router)
        .layer(DefaultBodyLimit::max(5 * 1024)) // 5 kB max request body limit
        .with_state(store);

    Ok(router.into())
}

#[derive(Debug, Serialize, Deserialize)]
struct CreateLinkBody {
    data: String,
}

async fn create_link(
    State(store): State<LinkStore>,
    Json(body): Json<CreateLinkBody>,
) -> AppResult<impl IntoResponse> {
    let mut store = store.write().await;

    // Remove keys to save memory
    if store.len() > MAX_LINKS {
        let key = store.keys().next().unwrap().clone();
        store.remove(&key);
    }

    // Make sure id does not already exist
    let mut id = nanoid::nanoid!(LINK_LENGTH);
    while store.contains_key(&id) {
        id = nanoid::nanoid!(LINK_LENGTH);
    }

    store.insert(
        id.clone(),
        Link {
            data: body.data,
            expire_time: Instant::now() + EXPIRE_DURATION,
        },
    );

    Ok((StatusCode::OK, id))
}

async fn delete_link(
    State(store): State<LinkStore>,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let exists = store.write().await.remove(&id).is_some();

    match exists {
        true => Ok(StatusCode::NO_CONTENT),
        false => Ok(StatusCode::NOT_FOUND),
    }
}

// Get data from the link and deletes it
async fn get_data_view(
    State(store): State<LinkStore>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let store = store.read().await;

    if let Some(link) = store.get(&id) {
        let expire_secs = (link.expire_time - Instant::now()).as_secs();
        let html = include_str!("view.html")
            .replace("%DATA%", &link.data)
            .replace("%EXPIRE_SECS%", &expire_secs.to_string());
        Ok((StatusCode::OK, cached_header!("text/html", 0), Html(html)))
    } else {
        Ok((
            StatusCode::NOT_FOUND,
            cached_header!("text/html", 31536000),
            Html(include_str!("static/not-found.html").to_owned()),
        ))
    }
}

fn cleanup_task(store: LinkStore) {
    // A separate background task to clean up
    tokio::spawn(async move {
        loop {
            let now = Instant::now();
            tokio::time::sleep(CLEANUP_INTERVAL).await;
            store
                .write()
                .await
                .retain(|_, link| link.expire_time - now > Duration::ZERO);
        }
    });
}

// Make our own error that wraps `anyhow::Error`.
#[derive(Debug)]
struct AppError(anyhow::Error);
type AppResult<T> = Result<T, AppError>;

// Tell axum how to convert `AppError` into a response.
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {}", self.0),
        )
            .into_response()
    }
}

// This enables using `?` on functions that return `Result<_, anyhow::Error>` to turn them into
// `Result<_, AppError>`. That way you don't need to do that manually.
impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

#[macro_export]
macro_rules! static_route {
    ( $file:expr, $ext:expr ) => {{
        get((
            cached_header!(concat!("text/", $ext), 31536000),
            include_str!(concat!("static/", $file, ".", $ext)),
        ))
    }};
}

#[macro_export]
macro_rules! cached_header {
    ($content_type: expr, $time: literal) => {
        [
            (header::CONTENT_TYPE, $content_type),
            (
                header::CACHE_CONTROL,
                concat!("public,max-age=", $time, "immutable"),
            ),
        ]
    };
}
