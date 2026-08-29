use std::sync::Arc;

use askama::Template;

use axum::{
    extract::{
        Multipart,
        State,
    },
    http::StatusCode,
    response::{
        Html,
        Redirect,
    },
    routing::{
        get,
        post,
    },
    Router,
};

use crate::{
    app::{
        AppState,
        GoogleClientState,
        RcloneState,
    },
    google,
};

#[allow(dead_code)]
pub struct AlertItem {
    pub level: &'static str,
    pub icon: &'static str,
    pub message: String,
    pub modal_target: &'static str,
}

#[allow(dead_code)]
pub struct StatusItem {
    pub icon: &'static str,
    pub label: &'static str,
    pub value: String,
}

#[allow(dead_code)]
pub struct SetupStep {
    pub icon: &'static str,
    pub title: &'static str,
    pub description: String,
    pub state_label: &'static str,
    pub state_class: &'static str,
    pub complete: bool,
    pub modal_target: &'static str,
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
    setup_steps: Vec<SetupStep>,
    setup_percent: u8,
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

#[allow(dead_code)]
#[derive(Template)]
#[template(
    path = "partials/setup-progress.html",
    config = "askama.toml"
)]
struct SetupProgressTemplate {
    setup_steps: Vec<SetupStep>,
    setup_percent: u8,
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
            "/ui/setup-progress",
            get(ui_setup_progress),
        )
        .route(
            "/setup/google-client/import",
            post(import_google_client),
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

    let google_client_state =
        state.google_client_state();

    let alerts = build_alerts(
        &rclone_state,
        &google_client_state,
    );

    let status_items = build_status_items(
        &rclone_state,
        &google_client_state,
    );

    let (
        setup_steps,
        setup_percent,
    ) = build_setup_progress(
        &rclone_state,
        &google_client_state,
    );

    let poll_rclone = should_poll_rclone(
        &rclone_state,
    );

    let template = DashboardTemplate {
        title: "BOREAL",
        active_page: "dashboard",
        alerts,
        status_items,
        setup_steps,
        setup_percent,
        poll_rclone,
    };

    render_template(
        &template,
    )
}

fn build_setup_progress(
    rclone_state: &RcloneState,
    google_client_state: &GoogleClientState,
) -> (Vec<SetupStep>, u8) {
    let rclone_step = match rclone_state {
        RcloneState::Initializing => SetupStep {
            icon: "bi-hourglass-split",
            title: "Install Rclone",
            description: "BOREAL is installing and verifying its private Rclone binary."
                .to_string(),
            state_label: "In progress",
            state_class: "text-bg-warning",
            complete: false,
            modal_target: "",
        },

        RcloneState::Ready(
            status,
        ) => SetupStep {
            icon: "bi-check-circle-fill",
            title: "Install Rclone",
            description: format!(
                "{} is installed and ready.",
                status.version
            ),
            state_label: "Complete",
            state_class: "text-bg-success",
            complete: true,
            modal_target: "",
        },

        RcloneState::Error(
            error,
        ) => SetupStep {
            icon: "bi-exclamation-triangle-fill",
            title: "Install Rclone",
            description: format!(
                "Rclone setup failed: {error}"
            ),
            state_label: "Needs attention",
            state_class: "text-bg-danger",
            complete: false,
            modal_target: "",
        },
    };

    let google_step = match google_client_state {
        GoogleClientState::NotConfigured => SetupStep {
            icon: "bi-key",
            title: "Configure Google Client ID",
            description:
                "Enable the Google Drive API, create a Desktop OAuth client, and import its JSON file."
                    .to_string(),
            state_label: "Set up",
            state_class: "text-bg-warning",
            complete: false,
            modal_target: "googleClientSetupModal",
        },

        GoogleClientState::Ready(
            _,
        ) => SetupStep {
            icon: "bi-check-circle-fill",
            title: "Configure Google Client ID",
            description:
                "Google Desktop OAuth credentials are stored in BOREAL's private conf directory."
                    .to_string(),
            state_label: "Complete",
            state_class: "text-bg-success",
            complete: true,
            modal_target: "",
        },

        GoogleClientState::Error(
            error,
        ) => SetupStep {
            icon: "bi-exclamation-triangle-fill",
            title: "Configure Google Client ID",
            description: format!(
                "The saved credentials are invalid: {error}"
            ),
            state_label: "Fix setup",
            state_class: "text-bg-danger",
            complete: false,
            modal_target: "googleClientSetupModal",
        },
    };

    let remote_step = SetupStep {
        icon: "bi-cloud-plus",
        title: "Configure a Remote",
        description:
            "Remote configuration is the next planned setup stage and is not implemented yet."
                .to_string(),
        state_label: "Pending",
        state_class: "text-bg-secondary",
        complete: false,
        modal_target: "",
    };

    let steps = vec![
        rclone_step,
        google_step,
        remote_step,
    ];

    let complete_count = steps
        .iter()
        .filter(
            |step| step.complete,
        )
        .count();

    let setup_percent = (
        complete_count * 100 / steps.len()
    ) as u8;

    (
        steps,
        setup_percent,
    )
}

async fn about(
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, StatusCode> {
    let rclone_state = state.rclone_state();

    let google_client_state =
        state.google_client_state();

    let alerts = build_alerts(
        &rclone_state,
        &google_client_state,
    );

    let status_items = build_status_items(
        &rclone_state,
        &google_client_state,
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

async fn status() -> StatusCode {
    StatusCode::NO_CONTENT
}

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

    let google_client_state =
        state.google_client_state();

    let template = AlertsTemplate {
        alerts: build_alerts(
            &rclone_state,
            &google_client_state,
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

    let google_client_state =
        state.google_client_state();

    let template = StatusTemplate {
        status_items: build_status_items(
            &rclone_state,
            &google_client_state,
        ),

        poll_rclone: should_poll_rclone(
            &rclone_state,
        ),
    };

    render_template(
        &template,
    )
}

async fn ui_setup_progress(
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, StatusCode> {
    let rclone_state = state.rclone_state();

    let google_client_state =
        state.google_client_state();

    let (
        setup_steps,
        setup_percent,
    ) = build_setup_progress(
        &rclone_state,
        &google_client_state,
    );

    let template = SetupProgressTemplate {
        setup_steps,
        setup_percent,
        poll_rclone: should_poll_rclone(
            &rclone_state,
        ),
    };

    render_template(
        &template,
    )
}

async fn import_google_client(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Redirect, StatusCode> {
    let mut credentials: Option<Vec<u8>> =
        None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(
            |error| {
                eprintln!(
                    "Unable to read Google Client ID upload: {error}"
                );

                StatusCode::BAD_REQUEST
            },
        )?
    {
        if field.name() != Some(
            "credentials",
        ) {
            continue;
        }

        let data = field
            .bytes()
            .await
            .map_err(
                |error| {
                    eprintln!(
                        "Unable to read uploaded Google Client ID file: {error}"
                    );

                    StatusCode::BAD_REQUEST
                },
            )?;

        credentials = Some(
            data.to_vec(),
        );

        break;
    }

    let data = credentials
        .ok_or(
            StatusCode::BAD_REQUEST,
        )?;

    match google::client::import(
        &state.runtime,
        &data,
    ) {
        Ok(
            config,
        ) => {
            println!(
                "Google Client ID imported: {}",
                config.client_id
            );

            state.set_google_client_state(
                GoogleClientState::Ready(
                    config,
                ),
            );

            Ok(
                Redirect::to(
                    "/",
                ),
            )
        }

        Err(
            error,
        ) => {
            let message = error.to_string();

            eprintln!(
                "Google Client ID import failed: {message}"
            );

            state.set_google_client_state(
                GoogleClientState::Error(
                    message,
                ),
            );

            Ok(
                Redirect::to(
                    "/",
                ),
            )
        }
    }
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
    google_client_state: &GoogleClientState,
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
                    modal_target: "",
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
                    modal_target: "",
                },
            );
        }
    }

    match google_client_state {
        GoogleClientState::NotConfigured => {
            alerts.push(
                AlertItem {
                    level: "warning",
                    icon: "bi-key",
                    message:
                        "Google Client ID is not configured"
                            .to_string(),
                    modal_target:
                        "googleClientSetupModal",
                },
            );
        }

        GoogleClientState::Ready(
            _,
        ) => {}

        GoogleClientState::Error(
            error,
        ) => {
            alerts.push(
                AlertItem {
                    level: "danger",
                    icon: "bi-key",
                    message: format!(
                        "Google Client ID configuration is invalid: {error}"
                    ),
                    modal_target:
                        "googleClientSetupModal",
                },
            );
        }
    }

    alerts
}

fn build_status_items(
    rclone_state: &RcloneState,
    google_client_state: &GoogleClientState,
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

    let client_id_value =
        match google_client_state {
            GoogleClientState::NotConfigured => {
                "Not configured".to_string()
            }

            GoogleClientState::Ready(
                _,
            ) => {
                "Configured".to_string()
            }

            GoogleClientState::Error(
                _,
            ) => {
                "Invalid".to_string()
            }
        };

    vec![
        StatusItem {
            icon: "bi-folder-symlink",
            label: "Rclone",
            value: rclone_value,
        },

        StatusItem {
            icon: "bi-key",
            label: "ClientID",
            value: client_id_value,
        },

        StatusItem {
            icon: "bi-cloud",
            label: "Remotes",
            value: "None".to_string(),
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
