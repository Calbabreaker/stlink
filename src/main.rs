use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use redis::Commands;
use serde::{Deserialize, Serialize};

#[shuttle_runtime::main]
async fn axum(
    #[shuttle_runtime::Secrets] secrets: shuttle_runtime::SecretStore,
) -> shuttle_axum::ShuttleAxum {
    let redis_url = secrets.get("REDIS_URL").expect("REDIS_URL was not set");
    let client = redis::Client::open(redis_url).map_err(anyhow::Error::new)?;

    let file_router = Router::new()
        .route("/script.js", static_route!("script", "js"))
        .route("/style.css", static_route!("style", "css"))
        .route("/", static_route!("index", "html"));

    let router = Router::new()
        .merge(file_router)
        .route("/", post(create_link))
        .route("/:id", get(get_data_view).delete(delete_link))
        .layer(DefaultBodyLimit::max(5 * 1024)) // 5 kB max request body limit
        .with_state(client);

    Ok(router.into())
}

#[derive(Debug, Serialize, Deserialize)]
struct CreateLinkBody {
    data: String,
}

async fn create_link(
    State(mut client): State<redis::Client>,
    Json(body): Json<CreateLinkBody>,
) -> AppResult<impl IntoResponse> {
    // Create a random id in base64
    let id = nanoid::nanoid!(6);
    let _: () = client.set_ex(&id, body.data, 5 * 60)?; // Expire after 5 minutes

    Ok((StatusCode::OK, id))
}

async fn delete_link(
    State(mut client): State<redis::Client>,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let result: redis::RedisResult<()> = client.del(id);

    match result.is_ok() {
        false => Ok(StatusCode::NOT_FOUND),
        true => Ok(StatusCode::NO_CONTENT),
    }
}

// Get data from the link and deletes it
async fn get_data_view(
    State(mut client): State<redis::Client>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let view_html = include_str!("view.html");

    let (data, seconds_till_expire): (Option<String>, u64) =
        redis::pipe().get(&id).ttl(&id).query(&mut client)?;

    if let Some(data) = data {
        let html = view_html
            .replace("%DATA%", &data)
            .replace("%TTL%", &seconds_till_expire.to_string());
        Ok((StatusCode::OK, cached_header!("text/html"), Html(html)))
    } else {
        Ok((
            StatusCode::NOT_FOUND,
            cached_header!("text/html"),
            Html(include_str!("static/not-found.html").to_owned()),
        ))
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

#[macro_export]
macro_rules! static_route {
    ( $file:expr, $ext:expr ) => {{
        get((
            cached_header!(concat!("text/", $ext)),
            include_str!(concat!("static/", $file, ".", $ext)),
        ))
    }};
}

#[macro_export]
macro_rules! cached_header {
    ($content_type: expr) => {
        [
            (header::CONTENT_TYPE, $content_type),
            (header::CACHE_CONTROL, "public,max-age=31536000,immutable"),
        ]
    };
}
