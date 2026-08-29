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
        GoogleRemotesState,
        GoogleClientState,
        RcloneState,
    },
    google,
    rclone::remotes::{
        RemoteKind,
        RemoteState,
    },
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
    pub value_class: &'static str,
    pub value_url: String,
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
    pub remote_actions: Vec<RemoteAction>,
}

#[allow(dead_code)]
pub struct RemoteAction {
    pub label: &'static str,
    pub action: &'static str,
    pub state_label: &'static str,
    pub state_class: &'static str,
    pub disabled: bool,
    pub detail: String,
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
            "/rclone-gui",
            get(open_rclone_gui),
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
            "/setup/remotes/my-drive-rw",
            post(setup_my_drive_rw),
        )
        .route(
            "/setup/remotes/my-drive-ro",
            post(setup_my_drive_ro),
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
    let google_remotes_state =
        state.google_remotes_state();

    let alerts = build_alerts(
        &rclone_state,
        &google_client_state,
    );

    let status_items = build_status_items(
        &rclone_state,
        &google_client_state,
        &google_remotes_state,
    );

    let (
        setup_steps,
        setup_percent,
    ) = build_setup_progress(
        &rclone_state,
        &google_client_state,
        &google_remotes_state,
    );

    let poll_rclone = should_poll_setup(
        &rclone_state,
        &google_remotes_state,
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
    google_remotes_state: &GoogleRemotesState,
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
            remote_actions: Vec::new(),
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
            remote_actions: Vec::new(),
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
            remote_actions: Vec::new(),
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
            remote_actions: Vec::new(),
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
            remote_actions: Vec::new(),
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
            remote_actions: Vec::new(),
        },
    };

    let remote_complete = matches!(google_remotes_state.rw, RemoteState::Ready)
        && matches!(google_remotes_state.ro, RemoteState::Ready);
    let remote_busy = matches!(google_remotes_state.rw, RemoteState::Configuring)
        || matches!(google_remotes_state.ro, RemoteState::Configuring);
    let prerequisites_ready = matches!(rclone_state, RcloneState::Ready(_))
        && matches!(google_client_state, GoogleClientState::Ready(_));

    let remote_step = SetupStep {
        icon: if remote_complete { "bi-check-circle-fill" } else { "bi-cloud-plus" },
        title: "Configure My Drive Remotes",
        description:
            "Authorize separate read/write and read-only Google Drive connections. Google opens a browser tab for each authorization."
                .to_string(),
        state_label: if remote_complete { "Complete" } else { "Set up" },
        state_class: if remote_complete { "text-bg-success" } else { "text-bg-warning" },
        complete: remote_complete,
        modal_target: "",
        remote_actions: vec![
            build_remote_action(
                "Setup My Drive RW",
                "/setup/remotes/my-drive-rw",
                &google_remotes_state.rw,
                prerequisites_ready,
                remote_busy,
            ),
            build_remote_action(
                "Setup My Drive RO",
                "/setup/remotes/my-drive-ro",
                &google_remotes_state.ro,
                prerequisites_ready,
                remote_busy,
            ),
        ],
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

fn build_remote_action(
    label: &'static str,
    action: &'static str,
    state: &RemoteState,
    prerequisites_ready: bool,
    remote_busy: bool,
) -> RemoteAction {
    let (state_label, state_class) = match state {
        RemoteState::Ready => ("Complete", "text-bg-success"),
        RemoteState::Configuring => ("Authorizing…", "text-bg-warning"),
        RemoteState::Conflict(_) => ("Conflict", "text-bg-danger"),
        RemoteState::Error(_) => ("Retry", "text-bg-danger"),
        RemoteState::Waiting => ("Waiting", "text-bg-secondary"),
        RemoteState::NotConfigured => ("Setup", "text-bg-primary"),
    };

    RemoteAction {
        label,
        action,
        state_label,
        state_class,
        disabled: !prerequisites_ready
            || remote_busy
            || matches!(state, RemoteState::Ready | RemoteState::Conflict(_)),
        detail: match state {
            RemoteState::Conflict(error) | RemoteState::Error(error) => error.clone(),
            RemoteState::Configuring =>
                "Complete the Google authorization in the browser tab opened by Rclone."
                    .to_string(),
            _ => String::new(),
        },
    }
}

async fn about(
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, StatusCode> {
    let rclone_state = state.rclone_state();

    let google_client_state =
        state.google_client_state();
    let google_remotes_state =
        state.google_remotes_state();

    let alerts = build_alerts(
        &rclone_state,
        &google_client_state,
    );

    let status_items = build_status_items(
        &rclone_state,
        &google_client_state,
        &google_remotes_state,
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

async fn open_rclone_gui(
    State(state): State<Arc<AppState>>,
) -> Result<Redirect, StatusCode> {
    let url = rclone_gui_url(
        &state.rclone_state(),
    );

    if url.is_empty() {
        return Err(
            StatusCode::SERVICE_UNAVAILABLE,
        );
    }

    Ok(
        Redirect::to(
            &url,
        ),
    )
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
    let google_remotes_state =
        state.google_remotes_state();

    let template = StatusTemplate {
        status_items: build_status_items(
            &rclone_state,
            &google_client_state,
            &google_remotes_state,
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
    let google_remotes_state =
        state.google_remotes_state();

    let (
        setup_steps,
        setup_percent,
    ) = build_setup_progress(
        &rclone_state,
        &google_client_state,
        &google_remotes_state,
    );

    let template = SetupProgressTemplate {
        setup_steps,
        setup_percent,
        poll_rclone: should_poll_setup(
            &rclone_state,
            &google_remotes_state,
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

            state.refresh_google_remotes_if_ready();

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

async fn setup_my_drive_rw(
    State(state): State<Arc<AppState>>,
) -> Result<Redirect, StatusCode> {
    start_remote_setup(state, RemoteKind::MyDriveRw)
}

async fn setup_my_drive_ro(
    State(state): State<Arc<AppState>>,
) -> Result<Redirect, StatusCode> {
    start_remote_setup(state, RemoteKind::MyDriveRo)
}

fn start_remote_setup(
    state: Arc<AppState>,
    kind: RemoteKind,
) -> Result<Redirect, StatusCode> {
    AppState::configure_google_remote(state, kind)
        .map_err(|error| {
            eprintln!("Unable to start {} setup: {error}", kind.label());
            StatusCode::CONFLICT
        })?;

    Ok(Redirect::to("/"))
}

fn should_poll_rclone(
    rclone_state: &RcloneState,
) -> bool {
    matches!(
        rclone_state,
        RcloneState::Initializing
    )
}

fn should_poll_setup(
    rclone_state: &RcloneState,
    remotes_state: &GoogleRemotesState,
) -> bool {
    should_poll_rclone(rclone_state)
        || matches!(remotes_state.rw, RemoteState::Configuring)
        || matches!(remotes_state.ro, RemoteState::Configuring)
}

fn rclone_gui_url(
    rclone_state: &RcloneState,
) -> String {
    match rclone_state {
        RcloneState::Ready(
            status,
        ) => status
            .gui_url
            .clone()
            .unwrap_or_default(),

        _ => String::new(),
    }
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
    google_remotes_state: &GoogleRemotesState,
) -> Vec<StatusItem> {
    let (
        rclone_value,
        rclone_value_class,
    ) = match rclone_state {
        RcloneState::Initializing => {
            (
                "Initializing...".to_string(),
                "text-warning",
            )
        }

        RcloneState::Ready(
            status,
        ) => {
            (
                status
                    .version
                    .strip_prefix(
                        "rclone ",
                    )
                    .unwrap_or(
                        &status.version,
                    )
                    .to_string(),
                "text-success",
            )
        }

        RcloneState::Error(
            _,
        ) => {
            (
                "Unavailable".to_string(),
                "text-danger",
            )
        }
    };

    let (
        client_id_value,
        client_id_value_class,
    ) =
        match google_client_state {
            GoogleClientState::NotConfigured => {
                (
                    "Not configured".to_string(),
                    "text-warning",
                )
            }

            GoogleClientState::Ready(
                _,
            ) => {
                (
                    "Configured".to_string(),
                    "text-success",
                )
            }

            GoogleClientState::Error(
                _,
            ) => {
                (
                    "Invalid".to_string(),
                    "text-danger",
                )
            }
        };

    let remote_count = [
        &google_remotes_state.rw,
        &google_remotes_state.ro,
    ]
    .into_iter()
    .filter(|state| matches!(state, RemoteState::Ready))
    .count();
    let remote_class = if remote_count == 2 {
        "text-success"
    } else {
        "text-warning"
    };

    vec![
        StatusItem {
            icon: "bi-folder-symlink",
            label: "Rclone",
            value: rclone_value,
            value_class: rclone_value_class,
            value_url: rclone_gui_url(
                rclone_state,
            ),
        },

        StatusItem {
            icon: "bi-key",
            label: "ClientID",
            value: client_id_value,
            value_class: client_id_value_class,
            value_url: String::new(),
        },

        StatusItem {
            icon: "bi-cloud",
            label: "Remotes",
            value: format!("{remote_count} of 2 configured"),
            value_class: remote_class,
            value_url: String::new(),
        },

        StatusItem {
            icon: "bi-database",
            label: "Metadata",
            value: "Not synchronized".to_string(),
            value_class: "text-warning",
            value_url: String::new(),
        },

        StatusItem {
            icon: "bi-info-circle",
            label: "BOREAL",
            value: format!(
                "v{}",
                env!(
                    "CARGO_PKG_VERSION"
                ),
            ),
            value_class: "text-success",
            value_url: String::new(),
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
