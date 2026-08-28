use std::sync::Arc;

use askama::Template;

use axum::{
    extract::State,
    http::StatusCode,
    response::Html,
    routing::{
        get,
        post,
    },
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
    poll_rclone: bool,
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
    poll_rclone: bool,
}

#[allow(dead_code)]
#[derive(Template)]
#[template(
    path = "partials/alerts.html",
    config = "askama.toml"
)]
struct AlertsTemplate {
    alerts: Vec<AlertItem>,
    poll_rclone: bool,
}

#[allow(dead_code)]
#[derive(Template)]
#[template(
    path = "partials/status.html",
    config = "askama.toml"
)]
struct StatusTemplate {
    status_items: Vec<StatusItem>,
    poll_rclone: bool,
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
        .route(
            "/ui/alerts",
            get(ui_alerts),
        )
        .route(
            "/ui/status",
            get(ui_status),
        )
        .route(
            "/app/quit",
            post(quit),
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

    let poll_rclone = should_poll_rclone(
        &rclone_state,
    );

    let template = DashboardTemplate {
        title: "BOREAL",
        active_page: "dashboard",
        alerts,
        status_items,
        poll_rclone,
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

    let poll_rclone = should_poll_rclone(
        &rclone_state,
    );

    let template = AboutTemplate {
        title: "About BOREAL",
        active_page: "about",
        alerts,
        status_items,
        poll_rclone,
    };

    render_template(
        &template,
    )
}

/// Lightweight heartbeat endpoint.
///
/// The browser periodically checks this endpoint so it can detect when
/// the BOREAL process has stopped.
async fn status() -> StatusCode {
    StatusCode::NO_CONTENT
}

/// Request graceful application shutdown from the WebUI.
async fn quit(
    State(state): State<Arc<AppState>>,
) -> StatusCode {
    println!(
        "Quit requested from WebUI."
    );

    state.request_shutdown();

    StatusCode::ACCEPTED
}

async fn ui_alerts(
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, StatusCode> {
    let rclone_state = state.rclone_state();

    let template = AlertsTemplate {
        alerts: build_alerts(
            &rclone_state,
        ),

        poll_rclone: should_poll_rclone(
            &rclone_state,
        ),
    };

    render_template(
        &template,
    )
}

async fn ui_status(
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, StatusCode> {
    let rclone_state = state.rclone_state();

    let template = StatusTemplate {
        status_items: build_status_items(
            &rclone_state,
        ),

        poll_rclone: should_poll_rclone(
            &rclone_state,
        ),
    };

    render_template(
        &template,
    )
}

fn should_poll_rclone(
    rclone_state: &RcloneState,
) -> bool {
    matches!(
        rclone_state,
        RcloneState::Initializing
    )
}

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
        ) => {}

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
            status
                .version
                .strip_prefix(
                    "rclone ",
                )
                .unwrap_or(
                    &status.version,
                )
                .to_string()
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