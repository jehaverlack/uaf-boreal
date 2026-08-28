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
    let rclone_state = state.rclone_state();

    let alerts = build_alerts(
        &rclone_state,
    );

    let status_items = build_status_items(
        &rclone_state,
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
    let rclone_state = state.rclone_state();

    let alerts = build_alerts(
        &rclone_state,
    );

    let status_items = build_status_items(
        &rclone_state,
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
/// Alerts represent actual application conditions rather than static
/// placeholders.
fn build_alerts(
    rclone_state: &RcloneState,
) -> Vec<AlertItem> {
    let mut alerts = Vec::new();

    match rclone_state {
        RcloneState::Initializing => {
            alerts.push(
                AlertItem {
                    level: "warning",
                    icon: "bi-hourglass-split",
                    message:
                        "BOREAL is initializing Rclone..."
                            .to_string(),
                },
            );
        }

        RcloneState::Ready(
            _,
        ) => {
            /*
             * Rclone is ready.
             *
             * No alert is necessary.
             */
        }

        RcloneState::Error(
            error,
        ) => {
            alerts.push(
                AlertItem {
                    level: "danger",
                    icon: "bi-exclamation-triangle",
                    message: format!(
                        "Rclone initialization failed: {error}"
                    ),
                },
            );
        }
    }

    alerts
}

/// Build the global status bar.
fn build_status_items(
    rclone_state: &RcloneState,
) -> Vec<StatusItem> {
    let rclone_value = match rclone_state {
        RcloneState::Initializing => {
            "Initializing...".to_string()
        }

        RcloneState::Ready(
            status,
        ) => {
            status.version.clone()
        }

        RcloneState::Error(
            _,
        ) => {
            "Unavailable".to_string()
        }
    };

    vec![
        StatusItem {
            icon: "bi-folder-symlink",
            label: "Rclone",
            value: rclone_value,
        },

        StatusItem {
            icon: "bi-cloud",
            label: "Remote",
            value: "None".to_string(),
        },

        StatusItem {
            icon: "bi-person",
            label: "User",
            value: "Not configured".to_string(),
        },

        StatusItem {
            icon: "bi-database",
            label: "Metadata",
            value: "Not synchronized".to_string(),
        },

        StatusItem {
            icon: "bi-info-circle",
            label: "BOREAL",
            value: env!(
                "CARGO_PKG_VERSION"
            )
            .to_string(),
        },
    ]
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