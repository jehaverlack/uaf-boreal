use std::sync::Arc;

use askama::Template;

use axum::{
    extract::State,
    http::StatusCode,
    response::Html,
    routing::get,
    Router,
};

use crate::app::{
    AppState,
    RcloneState,
};

#[allow(dead_code)]
pub struct AlertItem {
    pub level: &'static str,
    pub icon: &'static str,
    pub message: String,
}

#[allow(dead_code)]
pub struct StatusItem {
    pub icon: &'static str,
    pub label: &'static str,
    pub value: String,
}

#[allow(dead_code)]
#[derive(Template)]
#[template(
    path = "dashboard.html",
    config = "askama.toml"
)]
struct DashboardTemplate {
    title: &'static str,
    active_page: &'static str,
    alerts: Vec<AlertItem>,
    status_items: Vec<StatusItem>,
}

#[allow(dead_code)]
#[derive(Template)]
#[template(
    path = "about.html",
    config = "askama.toml"
)]
struct AboutTemplate {
    title: &'static str,
    active_page: &'static str,
    alerts: Vec<AlertItem>,
    status_items: Vec<StatusItem>,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/",
            get(index),
        )
        .route(
            "/about",
            get(about),
        )
        .route(
            "/status",
            get(status),
        )
}

async fn index(
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, StatusCode> {
    let alerts = build_alerts(
        &state,
    );

    let status_items = build_status_items(
        &state,
    );

    let template = DashboardTemplate {
        title: "BOREAL",
        active_page: "dashboard",
        alerts,
        status_items,
    };

    render_template(
        &template,
    )
}

async fn about(
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, StatusCode> {
    let alerts = build_alerts(
        &state,
    );

    let status_items = build_status_items(
        &state,
    );

    let template = AboutTemplate {
        title: "About BOREAL",
        active_page: "about",
        alerts,
        status_items,
    };

    render_template(
        &template,
    )
}

async fn status() -> &'static str {
    "BOREAL is running"
}

/// Build the global BOREAL alerts.
///
/// Only actual detected problems should appear here.
///
/// There are intentionally no default or placeholder alerts.
fn build_alerts(
    state: &AppState,
) -> Vec<AlertItem> {
    let mut alerts = Vec::new();

    match &state.rclone {
        RcloneState::Ready(_) => {
            /*
             * Rclone is working.
             *
             * No alert is necessary.
             */
        }

        RcloneState::Error(error) => {
            alerts.push(
                AlertItem {
                    level: "danger",
                    icon: "bi-exclamation-triangle",
                    message: format!(
                        "Rclone installation failed: {error}"
                    ),
                },
            );
        }
    }

    alerts
}

/// Build the global status bar.
fn build_status_items(
    state: &AppState,
) -> Vec<StatusItem> {
    let mut items = Vec::new();

    let rclone_value = match &state.rclone {
        RcloneState::Ready(status) => {
            status.version.clone()
        }

        RcloneState::Error(_) => {
            "Unavailable".to_string()
        }
    };

    items.push(
        StatusItem {
            icon: "bi-folder-symlink",
            label: "Rclone",
            value: rclone_value,
        },
    );

    items.push(
        StatusItem {
            icon: "bi-cloud",
            label: "Remote",
            value: "None".to_string(),
        },
    );

    items.push(
        StatusItem {
            icon: "bi-person",
            label: "User",
            value: "Not configured".to_string(),
        },
    );

    items.push(
        StatusItem {
            icon: "bi-database",
            label: "Metadata",
            value: "Not synchronized".to_string(),
        },
    );

    items.push(
        StatusItem {
            icon: "bi-info-circle",
            label: "BOREAL",
            value: env!(
                "CARGO_PKG_VERSION"
            )
            .to_string(),
        },
    );

    items
}

fn render_template<T>(
    template: &T,
) -> Result<Html<String>, StatusCode>
where
    T: Template,
{
    template
        .render()
        .map(
            Html,
        )
        .map_err(
            |error| {
                eprintln!(
                    "Unable to render HTML template: {error}"
                );

                StatusCode::INTERNAL_SERVER_ERROR
            },
        )
}