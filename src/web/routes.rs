use askama::Template;

use axum::{
    http::StatusCode,
    response::Html,
    routing::get,
    Router,
};

#[allow(dead_code)]
pub struct AlertItem<'a> {
    pub level: &'a str,
    pub icon: &'a str,
    pub message: &'a str,
}

#[allow(dead_code)]
pub struct StatusItem<'a> {
    pub icon: &'a str,
    pub label: &'a str,
    pub value: &'a str,
}

#[allow(dead_code)]
#[derive(Template)]
#[template(
    path = "dashboard.html",
    config = "askama.toml"
)]
struct DashboardTemplate<'a> {
    title: &'a str,
    active_page: &'a str,
    alerts: Vec<AlertItem<'a>>,
    status_items: Vec<StatusItem<'a>>,
}

#[allow(dead_code)]
#[derive(Template)]
#[template(
    path = "about.html",
    config = "askama.toml"
)]
struct AboutTemplate<'a> {
    title: &'a str,
    active_page: &'a str,
    alerts: Vec<AlertItem<'a>>,
    status_items: Vec<StatusItem<'a>>,
}

pub fn router() -> Router {
    Router::new()
        .route("/", get(index))
        .route("/about", get(about))
        .route("/status", get(status))
}

async fn index() -> Result<Html<String>, StatusCode> {
    /*
     * Placeholder application state.
     *
     * These values will eventually be populated from
     * BOREAL's actual system/configuration detection.
     */
    let alerts = vec![
        AlertItem {
            level: "warning",
            icon: "bi-exclamation-triangle",
            message: "Rclone status has not yet been checked",
        },
        AlertItem {
            level: "warning",
            icon: "bi-key",
            message: "Client ID status has not yet been checked",
        },
        AlertItem {
            level: "warning",
            icon: "bi-cloud",
            message: "Remote configuration has not yet been checked",
        },
    ];

    let status_items = vec![
        StatusItem {
            icon: "bi-folder-symlink",
            label: "Rclone",
            value: "Unknown",
        },
        StatusItem {
            icon: "bi-cloud",
            label: "Remote",
            value: "None",
        },
        StatusItem {
            icon: "bi-person",
            label: "User",
            value: "Not configured",
        },
        StatusItem {
            icon: "bi-database",
            label: "Metadata",
            value: "Not synchronized",
        },
        StatusItem {
            icon: "bi-info-circle",
            label: "BOREAL",
            value: env!("CARGO_PKG_VERSION"),
        },
    ];

    let template = DashboardTemplate {
        title: "BOREAL",
        active_page: "dashboard",
        alerts,
        status_items,
    };

    render_template(&template)
}

async fn about() -> Result<Html<String>, StatusCode> {
    /*
     * For now the About page receives minimal global
     * application state.
     *
     * Later this will come from shared application state
     * instead of being constructed by each route.
     */
    let alerts = Vec::new();

    let status_items = vec![
        StatusItem {
            icon: "bi-info-circle",
            label: "BOREAL",
            value: env!("CARGO_PKG_VERSION"),
        },
    ];

    let template = AboutTemplate {
        title: "About BOREAL",
        active_page: "about",
        alerts,
        status_items,
    };

    render_template(&template)
}

async fn status() -> &'static str {
    "BOREAL is running"
}

fn render_template<T>(
    template: &T,
) -> Result<Html<String>, StatusCode>
where
    T: Template,
{
    template
        .render()
        .map(Html)
        .map_err(|error| {
            eprintln!(
                "Unable to render HTML template: {error}"
            );

            StatusCode::INTERNAL_SERVER_ERROR
        })
}