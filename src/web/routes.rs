use axum::{
    response::Html,
    routing::get,
    Router,
};

pub fn router() -> Router {
    Router::new()
        .route("/", get(index))
        .route("/status", get(status))
}

async fn index() -> Html<&'static str> {
    Html(
        r#"<!doctype html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <meta
        name="viewport"
        content="width=device-width, initial-scale=1"
    >

    <title>BOREAL</title>
</head>

<body>
    <main>
        <h1>BOREAL</h1>

        <p>
            Browser-based Organizer for Rclone Exploration,
            Audit &amp; Lookup
        </p>

        <p>
            BOREAL is running.
        </p>
    </main>
</body>
</html>
"#,
    )
}

async fn status() -> &'static str {
    "BOREAL is running"
}