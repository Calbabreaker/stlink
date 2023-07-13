use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    extract::DefaultBodyLimit,
    extract::Path,
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Extension, Json, Router,
};
use base64::Engine;
use ring::rand::SystemRandom;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Link {
    data: String,
    created_at: Duration,
    encrypted: bool,
}

type LinkStore = Arc<RwLock<HashMap<String, Link>>>;
type LastMinute = Arc<RwLock<u64>>;

const EXPIRE_TIME: Duration = Duration::from_secs(60 * 5);

macro_rules! route_file {
    ($file:expr, $content_type:expr) => {
        || async {
            // is this a hack?
            ([(header::CONTENT_TYPE, $content_type)], include_str!($file))
        }
    };
}

#[shuttle_runtime::main]
async fn axum() -> shuttle_axum::ShuttleAxum {
    let router = Router::new()
        .route("/", get(|| async { Html(include_str!("index.html")) }))
        .route(
            "/script.js",
            get(route_file!("script.js", "text/javascript")),
        )
        .route("/style.css", get(route_file!("style.css", "text/css")))
        .route("/", post(create_link))
        .route("/:id", get(get_data_view))
        .layer(Extension(LinkStore::default()))
        .layer(Extension(SystemRandom::new()))
        .layer(Extension(LastMinute::default()))
        .layer(DefaultBodyLimit::max(15 * 1000)); // 15 kB max request body limit

    Ok(router.into())
}

#[derive(Debug, Serialize, Deserialize)]
struct CreateLinkBody {
    data: String,
    encrypted: bool,
}

async fn create_link(
    Extension(store_lock): Extension<LinkStore>,
    Extension(rand): Extension<SystemRandom>,
    Extension(last_minute_lock): Extension<LastMinute>,
    Json(body): Json<CreateLinkBody>,
) -> AppResult<impl IntoResponse> {
    // Create a random id in base64
    let random_bytes = ring::rand::generate::<[u8; 4]>(&rand)?.expose();
    let id = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(random_bytes);

    let now = SystemTime::now().duration_since(UNIX_EPOCH)?;
    let current_minute = now.as_secs() / 60;

    // Every minute, check the map and remove all links that are past expire time
    if let Ok(mut last_minute) = last_minute_lock.try_write() {
        if current_minute != *last_minute {
            store_lock
                .write()
                .unwrap()
                .retain(|_, link| now - link.created_at < EXPIRE_TIME);
            *last_minute = current_minute;
            println!("Removed some");
        }
    }

    store_lock.write().unwrap().insert(
        id.clone(),
        Link {
            encrypted: body.encrypted,
            data: body.data,
            created_at: now,
        },
    );

    Ok((StatusCode::OK, id))
}

// Get data from the link and deletes it
async fn get_data_view(
    Extension(store): Extension<LinkStore>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let view_html = include_str!("view.html");

    match store.write().unwrap().remove(&id) {
        Some(link) => {
            let html = view_html.replacen("%DATA%", &link.data, 1);
            let html = html.replacen(
                "%ENCRYPTED%",
                if link.encrypted { "true" } else { "false" },
                1,
            );
            (StatusCode::OK, Html(html))
        }
        None => (
            StatusCode::NOT_FOUND,
            Html(include_str!("not-found.html").to_owned()),
        ),
    }
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
