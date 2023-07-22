use std::time::Duration;

use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::Executor;

#[derive(Debug)]
struct Link {
    data: String,
    age: Option<sqlx::postgres::types::PgInterval>,
    encrypted: bool,
}

#[shuttle_runtime::main]
async fn axum(
    #[shuttle_shared_db::Postgres(local_uri = "postgres://postgres:@localhost/stlink")]
    pool: sqlx::PgPool,
) -> shuttle_axum::ShuttleAxum {
    pool.execute(include_str!("schema.sql"))
        .await
        .map_err(anyhow::Error::new)?;

    let file_router = route_files![("script", "js"), ("style", "css")];

    let router = Router::new()
        .route("/", get(|| async { Html(include_str!("index.html")) }))
        .merge(file_router)
        // Perhaps we should use static routes?
        .route("/", post(create_link))
        .route("/:id", get(get_data_view).delete(delete_link))
        .layer(DefaultBodyLimit::max(15 * 1000)) // 15 kB max request body limit
        .with_state(pool.clone());

    // Check the database for expired links every second
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            // Check the db and remove all links that are past expire time
            sqlx::query!("DELETE FROM links WHERE created_at < (NOW() - INTERVAL '5 MINUTE')")
                .execute(&pool)
                .await
                .ok();
        }
    });

    Ok(router.into())
}

#[derive(Debug, Serialize, Deserialize)]
struct CreateLinkBody {
    data: String,
    encrypted: bool,
}

async fn create_link(
    State(pool): State<sqlx::PgPool>,
    Json(body): Json<CreateLinkBody>,
) -> AppResult<impl IntoResponse> {
    // Create a random id in base64
    let id = nanoid::nanoid!(6);

    sqlx::query!(
        "INSERT INTO links(id, data, encrypted) values ($1, $2, $3)",
        id.clone(),
        body.data,
        body.encrypted
    )
    .execute(&pool)
    .await?;

    Ok((StatusCode::OK, id))
}

async fn delete_link(
    State(pool): State<sqlx::PgPool>,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let result = sqlx::query!("DELETE FROM links WHERE id=$1", id)
        .execute(&pool)
        .await?;

    match result.rows_affected() {
        0 => Ok(StatusCode::NOT_FOUND),
        _ => Ok(StatusCode::NO_CONTENT),
    }
}

// Get data from the link and deletes it
async fn get_data_view(
    State(pool): State<sqlx::PgPool>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let view_html = include_str!("view.html");

    let result = sqlx::query_as!(
        Link,
        "SELECT NOW() - created_at AS age,encrypted,data FROM links WHERE id=$1",
        id
    )
    .fetch_optional(&pool)
    .await?;

    match result {
        Some(link) => {
            let age_secs = link.age.unwrap().microseconds / 1000000;

            let html = view_html
                .replace("%DATA%", &link.data)
                .replace("%ENCRYPTED%", &link.encrypted.to_string())
                .replace("%AGE%", &age_secs.to_string());
            Ok((StatusCode::OK, Html(html)))
        }
        None => Ok((
            StatusCode::NOT_FOUND,
            Html(include_str!("not-found.html").to_owned()),
        )),
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
// Don't look at this
macro_rules! route_files {
    ( $( ($file:expr, $ext:expr) ),* ) => {
        {
            let mut router = Router::new();
            $(
                router = router.route(concat!("/",$file,".",$ext), get(|| async {
                (
                    [
                        (header::CONTENT_TYPE, concat!("text/", $ext)),
                        (header::CACHE_CONTROL, "public,max-age=31536000,immutable"),
                    ],
                    include_str!(concat!($file,".",$ext)),
                )
            }));
            )*
            router
        }
    };
}
