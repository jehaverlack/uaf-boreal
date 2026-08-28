use axum::{
    response::Html,
    routing::get,
    Router,
};

pub fn router() -> Router {
    Router::new()
        .route("/", get(index))
        .route("/about", get(about))
        .route("/status", get(status))
}

async fn index() -> Html<&'static str> {
    Html(
        r##"<!doctype html>
<html lang="en" data-bs-theme="light">

<head>
    <meta charset="utf-8">

    <meta
        name="viewport"
        content="width=device-width, initial-scale=1"
    >

    <title>BOREAL</title>

    <link
        href="https://cdn.jsdelivr.net/npm/bootstrap@5.3.8/dist/css/bootstrap.min.css"
        rel="stylesheet"
    >
</head>

<body>

<nav class="navbar navbar-expand-lg bg-body-tertiary border-bottom">

    <div class="container-fluid">

        <a
            class="navbar-brand fw-semibold"
            href="/"
        >
            BOREAL
        </a>

        <button
            class="navbar-toggler"
            type="button"
            data-bs-toggle="collapse"
            data-bs-target="#borealNavbar"
            aria-controls="borealNavbar"
            aria-expanded="false"
            aria-label="Toggle navigation"
        >
            <span class="navbar-toggler-icon"></span>
        </button>

        <div
            class="collapse navbar-collapse"
            id="borealNavbar"
        >

            <ul class="navbar-nav me-auto mb-2 mb-lg-0">

                <li class="nav-item">
                    <a
                        class="nav-link active"
                        aria-current="page"
                        href="/"
                    >
                        Dashboard
                    </a>
                </li>

                <li class="nav-item">
                    <a
                        class="nav-link"
                        href="/rclone"
                    >
                        Rclone
                    </a>
                </li>

                <li class="nav-item">
                    <a
                        class="nav-link"
                        href="/drives"
                    >
                        Drives
                    </a>
                </li>

                <li class="nav-item">
                    <a
                        class="nav-link"
                        href="/explorer"
                    >
                        Explorer
                    </a>
                </li>

                <li class="nav-item">
                    <a
                        class="nav-link"
                        href="/sharing"
                    >
                        Sharing
                    </a>
                </li>

                <li class="nav-item">
                    <a
                        class="nav-link"
                        href="/migration"
                    >
                        Migration
                    </a>
                </li>

            </ul>

            <ul class="navbar-nav ms-auto">

                <li class="nav-item dropdown">

                    <a
                        class="nav-link dropdown-toggle"
                        href="#"
                        role="button"
                        data-bs-toggle="dropdown"
                        aria-expanded="false"
                    >
                        App
                    </a>

                    <ul
                        class="dropdown-menu dropdown-menu-end"
                    >

                        <li>
                            <a
                                class="dropdown-item"
                                href="/about"
                            >
                                About
                            </a>
                        </li>

                    </ul>

                </li>

            </ul>

        </div>

    </div>

</nav>

<main class="container-fluid py-4">

    <div class="row">

        <div class="col">

            <h1 class="h3">
                BOREAL
            </h1>

            <p class="text-body-secondary">
                Browser-based Organizer for Rclone Exploration,
                Audit &amp; Lookup
            </p>

            <div class="card">

                <div class="card-body">

                    <h2 class="h5 card-title">
                        System Status
                    </h2>

                    <p class="card-text">
                        BOREAL is running.
                    </p>

                </div>

            </div>

        </div>

    </div>

</main>

<script
    src="https://cdn.jsdelivr.net/npm/bootstrap@5.3.8/dist/js/bootstrap.bundle.min.js"
></script>

</body>

</html>
"##,
    )
}

async fn about() -> Html<&'static str> {
    Html(
        r##"<!doctype html>
<html lang="en" data-bs-theme="light">

<head>
    <meta charset="utf-8">

    <meta
        name="viewport"
        content="width=device-width, initial-scale=1"
    >

    <title>About BOREAL</title>

    <link
        href="https://cdn.jsdelivr.net/npm/bootstrap@5.3.8/dist/css/bootstrap.min.css"
        rel="stylesheet"
    >
</head>

<body>

<nav class="navbar navbar-expand-lg bg-body-tertiary border-bottom">

    <div class="container-fluid">

        <a
            class="navbar-brand fw-semibold"
            href="/"
        >
            BOREAL
        </a>

        <div class="ms-auto">

            <a
                class="btn btn-outline-secondary btn-sm"
                href="/"
            >
                Back
            </a>

        </div>

    </div>

</nav>

<main class="container py-4">

    <h1 class="h3">
        About BOREAL
    </h1>

    <p>
        Browser-based Organizer for Rclone Exploration,
        Audit &amp; Lookup
    </p>

    <p class="text-body-secondary">
        BOREAL is a local desktop application for exploring,
        auditing, and organizing data managed through Rclone.
    </p>

</main>

<script
    src="https://cdn.jsdelivr.net/npm/bootstrap@5.3.8/dist/js/bootstrap.bundle.min.js"
></script>

</body>

</html>
"##,
    )
}

async fn status() -> &'static str {
    "BOREAL is running"
}