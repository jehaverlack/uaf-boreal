use std::sync::Arc;

use askama::Template;

use axum::{
    extract::{
        Form,
        Multipart,
        Query,
        State,
    },
    http::StatusCode,
    response::{
        Html,
        IntoResponse,
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
        MetadataState,
        RcloneState,
    },
    database::{
        self,
        settings::{
            self,
            InventorySettings,
        },
    },
    google,
    rclone::{
        self,
        remotes::{
            RemoteKind,
            RemoteState,
        },
    },
};

fn remote_state_label(state: &RemoteState) -> &'static str {
    match state {
        RemoteState::Ready => "Ready",
        RemoteState::Configuring => "Configuring",
        RemoteState::NotConfigured | RemoteState::Waiting => "Not ready",
        RemoteState::Conflict(_) => "Conflict",
        RemoteState::Error(_) => "Error",
    }
}

fn remote_state_class(state: &RemoteState) -> &'static str {
    match state {
        RemoteState::Ready => "text-bg-success",
        RemoteState::Configuring | RemoteState::Waiting => "text-bg-warning",
        RemoteState::NotConfigured => "text-bg-secondary",
        RemoteState::Conflict(_) | RemoteState::Error(_) => "text-bg-danger",
    }
}

fn configured_remote_count(
    runtime: &crate::bootstrap::Runtime,
    rclone_state: &RcloneState,
) -> usize {
    match rclone_state {
        RcloneState::Ready(status) => rclone::remotes::list_configured(runtime, &status.path)
            .map(|remotes| remotes.len())
            .unwrap_or(0),
        _ => 0,
    }
}

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
    pub spinner: bool,
    pub age_timestamp: String,
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
pub struct MetadataView {
    pub available: bool,
    pub poll: bool,
    pub updating: bool,
    pub state_label: String,
    pub state_class: &'static str,
    pub phase: String,
    pub files_scanned: u64,
    pub folders_scanned: u64,
    pub permissions_scanned: u64,
    pub size_label: String,
    pub errors: u64,
    pub completed_at: String,
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
    metadata: MetadataView,
}

#[allow(dead_code)]
#[derive(Template)]
#[template(
    path = "settings.html",
    config = "askama.toml"
)]
struct SettingsTemplate {
    title: &'static str,
    active_page: &'static str,
    alerts: Vec<AlertItem>,
    status_items: Vec<StatusItem>,
    poll_rclone: bool,
    settings: InventorySettings,
    saved: bool,
    error: String,
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
pub struct RemoteView {
    pub name: String,
    pub backend: String,
    pub access: &'static str,
    pub purpose: &'static str,
    pub status: &'static str,
    pub status_class: &'static str,
}

#[allow(dead_code)]
#[derive(Template)]
#[template(
    path = "remotes.html",
    config = "askama.toml"
)]
struct RemotesTemplate {
    title: &'static str,
    active_page: &'static str,
    alerts: Vec<AlertItem>,
    status_items: Vec<StatusItem>,
    poll_rclone: bool,
    remotes: Vec<RemoteView>,
    error: String,
}

#[allow(dead_code)]
pub struct DriveExplorerRow {
    pub item_id: String,
    pub name: String,
    pub is_directory: bool,
    pub name_url: String,
    pub name_new_tab: bool,
    pub mime_type: String,
    pub type_icon: &'static str,
    pub tags: Vec<TagPill>,
    pub permissions: String,
    pub size: String,
    pub modified_at: String,
    pub owner_email: String,
    pub drive_url: String,
}

#[allow(dead_code)]
pub struct TagPill {
    pub name: String,
    pub color: String,
    pub text_color: &'static str,
}

#[allow(dead_code)]
#[derive(Template)]
#[template(
    path = "my-drive.html",
    config = "askama.toml"
)]
struct MyDriveTemplate {
    title: &'static str,
    active_page: &'static str,
    alerts: Vec<AlertItem>,
    status_items: Vec<StatusItem>,
    poll_rclone: bool,
    current_path: String,
    parent_path: String,
    has_parent: bool,
    rows: Vec<DriveExplorerRow>,
    error: String,
    search: String,
    sort: String,
    direction: String,
    name_sort_url: String,
    type_sort_url: String,
    size_sort_url: String,
    modified_sort_url: String,
    owner_sort_url: String,
    clear_search_url: String,
    tags: Vec<database::inventory::Tag>,
    tag_filter: String,
    tagged_count: usize,
}

#[allow(dead_code)]
#[derive(Template)]
#[template(path = "tags.html", config = "askama.toml")]
struct TagsTemplate {
    title: &'static str,
    active_page: &'static str,
    alerts: Vec<AlertItem>,
    status_items: Vec<StatusItem>,
    poll_rclone: bool,
    tags: Vec<database::inventory::Tag>,
    saved: bool,
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

#[allow(dead_code)]
#[derive(Template)]
#[template(
    path = "partials/metadata-progress.html",
    config = "askama.toml"
)]
struct MetadataProgressTemplate {
    metadata: MetadataView,
}

#[allow(dead_code)]
#[derive(Template)]
#[template(
    path = "partials/drive-summaries.html",
    config = "askama.toml"
)]
struct DriveSummariesTemplate {
    metadata: MetadataView,
}

#[derive(serde::Deserialize)]
struct SettingsQuery {
    #[serde(default)]
    saved: bool,
}

#[derive(serde::Deserialize, Default)]
struct DrivePathQuery {
    #[serde(default)]
    path: String,
    #[serde(default)]
    q: String,
    #[serde(default)]
    sort: String,
    #[serde(default)]
    direction: String,
    #[serde(default)]
    tag: String,
    #[serde(default)]
    tagged: usize,
}

#[derive(serde::Deserialize)]
struct ApplyTagForm {
    #[serde(default)]
    selected_item_ids: String,
    tag: String,
    path: String,
    q: String,
    sort: String,
    direction: String,
    tag_filter: String,
}

#[derive(serde::Deserialize)]
struct TagForm {
    #[serde(default)]
    slug: String,
    name: String,
    color: String,
}

#[derive(serde::Deserialize)]
struct SettingsForm {
    #[serde(default)]
    automatic_updates: Option<String>,
    refresh_interval_hours: u32,
    full_reconciliation_days: u32,
    #[serde(default)]
    update_when_overdue_at_startup: Option<String>,
    #[serde(default)]
    permission_scanning: Option<String>,
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
            "/remotes",
            get(remotes_page),
        )
        .route(
            "/my-drive",
            get(my_drive_page),
        )
        .route(
            "/my-drive/tags",
            post(apply_my_drive_tag),
        )
        .route(
            "/tags",
            get(tags_page),
        )
        .route(
            "/tags/create",
            post(create_tag),
        )
        .route(
            "/tags/update",
            post(update_tag),
        )
        .route(
            "/settings",
            get(settings_page).post(save_settings),
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
            "/ui/metadata-progress",
            get(ui_metadata_progress),
        )
        .route(
            "/ui/drive-summaries",
            get(ui_drive_summaries),
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
            "/metadata/update",
            post(start_metadata_update),
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
    let metadata_state =
        state.metadata_state();

    let alerts = build_alerts(
        &rclone_state,
        &google_client_state,
    );

    let status_items = build_status_items(
        &rclone_state,
        &google_client_state,
        &google_remotes_state,
        &metadata_state,
        configured_remote_count(&state.runtime, &rclone_state),
    );

    let (
        setup_steps,
        setup_percent,
    ) = build_setup_progress(
        &rclone_state,
        &google_client_state,
        &google_remotes_state,
    );

    let poll_rclone = should_poll_ui(
        &rclone_state,
        &google_remotes_state,
        &metadata_state,
    );

    let template = DashboardTemplate {
        title: "BOREAL",
        active_page: "dashboard",
        alerts,
        status_items,
        setup_steps,
        setup_percent,
        poll_rclone,
        metadata: build_metadata_view(
            &metadata_state,
            setup_percent == 100,
            should_poll_setup(
                &rclone_state,
                &google_remotes_state,
            ),
        ),
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

fn build_metadata_view(
    state: &MetadataState,
    available: bool,
    poll_for_setup: bool,
) -> MetadataView {
    match state {
        MetadataState::NotSynchronized => MetadataView {
            available,
            poll: poll_for_setup,
            updating: false,
            state_label: "Not synchronized".to_string(),
            state_class: "text-bg-warning",
            phase: "No metadata inventory has been created yet.".to_string(),
            files_scanned: 0,
            folders_scanned: 0,
            permissions_scanned: 0,
            size_label: "0 B".to_string(),
            errors: 0,
            completed_at: String::new(),
        },

        MetadataState::Updating(
            progress,
        ) => MetadataView {
            available,
            poll: true,
            updating: true,
            state_label: "Updating".to_string(),
            state_class: "text-bg-primary",
            phase: progress.phase.to_string(),
            files_scanned: progress.files_scanned,
            folders_scanned: progress.folders_scanned,
            permissions_scanned: progress.permissions_scanned,
            size_label: format_bytes(
                progress.bytes_discovered,
            ),
            errors: progress.errors,
            completed_at: String::new(),
        },

        MetadataState::Synchronized(
            summary,
        ) => MetadataView {
            available,
            poll: poll_for_setup,
            updating: false,
            state_label: "Synchronized".to_string(),
            state_class: "text-bg-success",
            phase: "My Drive inventory is current as of the completed update.".to_string(),
            files_scanned: summary.files_scanned,
            folders_scanned: summary.folders_scanned,
            permissions_scanned: summary.permissions_scanned,
            size_label: format_bytes(
                summary.bytes_discovered,
            ),
            errors: 0,
            completed_at: summary.completed_at.clone(),
        },

        MetadataState::Error(
            error,
        ) => MetadataView {
            available,
            poll: poll_for_setup,
            updating: false,
            state_label: "Update failed".to_string(),
            state_class: "text-bg-danger",
            phase: error.clone(),
            files_scanned: 0,
            folders_scanned: 0,
            permissions_scanned: 0,
            size_label: "0 B".to_string(),
            errors: 1,
            completed_at: String::new(),
        },
    }
}

fn format_bytes(
    bytes: u64,
) -> String {
    const GB: f64 = 1_000_000_000.0;
    const MB: f64 = 1_000_000.0;

    if bytes as f64 >= GB {
        format!(
            "{:.1} GB",
            bytes as f64 / GB,
        )
    } else if bytes as f64 >= MB {
        format!(
            "{:.1} MB",
            bytes as f64 / MB,
        )
    } else {
        format!(
            "{bytes} B",
        )
    }
}

async fn settings_page(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SettingsQuery>,
) -> Result<Html<String>, StatusCode> {
    let database = state.database()
        .map_err(
            |error| {
                eprintln!(
                    "Unable to open settings: {error}"
                );
                StatusCode::SERVICE_UNAVAILABLE
            },
        )?;
    let inventory_settings = settings::load(
        &database,
    )
    .map_err(
        |error| {
            eprintln!(
                "Unable to load settings: {error}"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        },
    )?;

    render_settings(
        &state,
        inventory_settings,
        query.saved,
        String::new(),
    )
}

async fn save_settings(
    State(state): State<Arc<AppState>>,
    Form(form): Form<SettingsForm>,
) -> Result<axum::response::Response, StatusCode> {
    let inventory_settings = InventorySettings {
        automatic_updates: form.automatic_updates.is_some(),
        refresh_interval_hours: form.refresh_interval_hours,
        full_reconciliation_days: form.full_reconciliation_days,
        update_when_overdue_at_startup:
            form.update_when_overdue_at_startup.is_some(),
        permission_scanning: form.permission_scanning.is_some(),
    };
    let database = state.database()
        .map_err(
            |_| StatusCode::SERVICE_UNAVAILABLE,
        )?;

    match settings::save(
        &database,
        &inventory_settings,
    ) {
        Ok(
            (),
        ) => Ok(
            Redirect::to(
                "/settings?saved=true",
            )
            .into_response(),
        ),

        Err(
            error,
        ) => render_settings(
            &state,
            inventory_settings,
            false,
            error.to_string(),
        )
        .map(
            axum::response::IntoResponse::into_response,
        ),
    }
}

fn render_settings(
    state: &AppState,
    inventory_settings: InventorySettings,
    saved: bool,
    error: String,
) -> Result<Html<String>, StatusCode> {
    let rclone_state = state.rclone_state();
    let google_client_state = state.google_client_state();
    let google_remotes_state = state.google_remotes_state();
    let metadata_state = state.metadata_state();

    let template = SettingsTemplate {
        title: "Settings - BOREAL",
        active_page: "settings",
        alerts: build_alerts(
            &rclone_state,
            &google_client_state,
        ),
        status_items: build_status_items(
            &rclone_state,
            &google_client_state,
            &google_remotes_state,
            &metadata_state,
            configured_remote_count(&state.runtime, &rclone_state),
        ),
        poll_rclone: should_poll_ui(
            &rclone_state,
            &google_remotes_state,
            &metadata_state,
        ),
        settings: inventory_settings,
        saved,
        error,
    };

    render_template(
        &template,
    )
}

async fn about(
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, StatusCode> {
    let rclone_state = state.rclone_state();

    let google_client_state =
        state.google_client_state();
    let google_remotes_state =
        state.google_remotes_state();
    let metadata_state =
        state.metadata_state();

    let alerts = build_alerts(
        &rclone_state,
        &google_client_state,
    );

    let status_items = build_status_items(
        &rclone_state,
        &google_client_state,
        &google_remotes_state,
        &metadata_state,
        configured_remote_count(&state.runtime, &rclone_state),
    );

    let poll_rclone = should_poll_ui(
        &rclone_state,
        &google_remotes_state,
        &metadata_state,
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

async fn remotes_page(
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, StatusCode> {
    let rclone_state = state.rclone_state();
    let google_client_state = state.google_client_state();
    let google_remotes_state = state.google_remotes_state();
    let metadata_state = state.metadata_state();

    let listed = match &rclone_state {
        RcloneState::Ready(status) => rclone::remotes::list_configured(
            &state.runtime,
            &status.path,
        ),
        _ => Err("Rclone is not ready".into()),
    };
    let (remotes, error) = match listed {
        Ok(remotes) => (
            remotes.into_iter().map(|remote| {
                let (access, purpose, status, status_class) = match remote.name.as_str() {
                    "my-drive-ro" => (
                        "Read only",
                        "Metadata inventory",
                        remote_state_label(&google_remotes_state.ro),
                        remote_state_class(&google_remotes_state.ro),
                    ),
                    "my-drive-rw" => (
                        "Read/write",
                        "Migration operations",
                        remote_state_label(&google_remotes_state.rw),
                        remote_state_class(&google_remotes_state.rw),
                    ),
                    _ => ("Remote-defined", "General", "Configured", "text-bg-success"),
                };
                RemoteView {
                    name: remote.name,
                    backend: remote.backend,
                    access,
                    purpose,
                    status,
                    status_class,
                }
            }).collect(),
            String::new(),
        ),
        Err(error) => {
            eprintln!("Unable to render remotes page: {error}");
            (Vec::new(), error.to_string())
        }
    };

    let template = RemotesTemplate {
        title: "Remotes - BOREAL",
        active_page: "remotes",
        alerts: build_alerts(&rclone_state, &google_client_state),
        status_items: build_status_items(
            &rclone_state,
            &google_client_state,
            &google_remotes_state,
            &metadata_state,
            remotes.len(),
        ),
        poll_rclone: should_poll_ui(&rclone_state, &google_remotes_state, &metadata_state),
        remotes,
        error,
    };
    render_template(&template)
}

async fn my_drive_page(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DrivePathQuery>,
) -> Result<Html<String>, StatusCode> {
    let rclone_state = state.rclone_state();
    let google_client_state = state.google_client_state();
    let google_remotes_state = state.google_remotes_state();
    let metadata_state = state.metadata_state();
    let database = state.database().map_err(|error| {
        eprintln!("Unable to open My Drive explorer: {error}");
        StatusCode::SERVICE_UNAVAILABLE
    })?;
    let has_parent = !query.path.is_empty();
    let parent_filter = has_parent.then_some(query.path.as_str());
    let sort = match query.sort.as_str() {
        "type" | "size" | "modified" | "owner" => query.sort.as_str(),
        _ => "name",
    };
    let descending = query.direction == "desc";
    let (items, error) = match database::inventory::list_my_drive_directory(
        &database,
        parent_filter,
        &query.q,
        &query.tag,
        sort,
        descending,
    ) {
        Ok(items) => (items, String::new()),
        Err(error) => {
            eprintln!("Unable to list My Drive explorer directory: {error}");
            (Vec::new(), error.to_string())
        }
    };
    let rows = items.into_iter().map(|item| DriveExplorerRow {
        drive_url: if item.is_directory {
            format!("https://drive.google.com/drive/folders/{}", item.item_id)
        } else {
            format!("https://drive.google.com/open?id={}", item.item_id)
        },
        item_id: item.item_id.clone(),
        name: item.name,
        name_url: if item.is_directory {
            explorer_url(&item.relative_path, "", &query.tag, sort, if descending { "desc" } else { "asc" })
        } else {
            format!("https://drive.google.com/open?id={}", item.item_id)
        },
        name_new_tab: !item.is_directory,
        is_directory: item.is_directory,
        type_icon: mime_icon(item.is_directory, item.mime_type.as_deref()),
        mime_type: if item.is_directory { "Folder".to_string() } else { item.mime_type.unwrap_or_else(|| "Unknown file type".to_string()) },
        tags: item.tags.into_iter().map(|tag| TagPill {
            text_color: tag_text_color(&tag.color),
            name: tag.name,
            color: tag.color,
        }).collect(),
        permissions: if item.permissions.is_empty() {
            "—".to_string()
        } else {
            item.permissions.join(", ")
        },
        size: item.size_bytes.map(format_bytes).unwrap_or_else(|| "—".to_string()),
        modified_at: item.modified_at.unwrap_or_else(|| "—".to_string()),
        owner_email: item.owner_email.unwrap_or_else(|| "—".to_string()),
    }).collect();
    let parent_path = query.path.rsplit_once('/')
        .map(|(parent, _)| parent.to_string())
        .unwrap_or_default();
    let tags = database::inventory::list_tags(&database).map_err(|error| {
        eprintln!("Unable to load My Drive tags: {error}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let template = MyDriveTemplate {
        title: "My Drive - BOREAL",
        active_page: "my-drive",
        alerts: build_alerts(&rclone_state, &google_client_state),
        status_items: build_status_items(
            &rclone_state,
            &google_client_state,
            &google_remotes_state,
            &metadata_state,
            configured_remote_count(&state.runtime, &rclone_state),
        ),
        poll_rclone: should_poll_ui(&rclone_state, &google_remotes_state, &metadata_state),
        current_path: if query.path.is_empty() { "My Drive".to_string() } else { query.path.clone() },
        parent_path,
        has_parent,
        rows,
        error,
        search: query.q.clone(),
        sort: sort.to_string(),
        direction: if descending { "desc".to_string() } else { "asc".to_string() },
        name_sort_url: sort_url(&query.path, &query.q, &query.tag, sort, descending, "name"),
        type_sort_url: sort_url(&query.path, &query.q, &query.tag, sort, descending, "type"),
        size_sort_url: sort_url(&query.path, &query.q, &query.tag, sort, descending, "size"),
        modified_sort_url: sort_url(&query.path, &query.q, &query.tag, sort, descending, "modified"),
        owner_sort_url: sort_url(&query.path, &query.q, &query.tag, sort, descending, "owner"),
        clear_search_url: explorer_url(
            &query.path,
            "",
            &query.tag,
            sort,
            if descending { "desc" } else { "asc" },
        ),
        tags,
        tag_filter: query.tag,
        tagged_count: query.tagged,
    };
    render_template(&template)
}

fn mime_icon(is_directory: bool, mime_type: Option<&str>) -> &'static str {
    if is_directory {
        "bi-folder-fill"
    } else {
        match mime_type.unwrap_or("") {
            value if value.contains("spreadsheet") || value.contains("excel") => "bi-file-earmark-spreadsheet",
            value if value.contains("presentation") || value.contains("powerpoint") => "bi-file-earmark-slides",
            value if value.contains("document") || value.contains("word") || value.starts_with("text/") => "bi-file-earmark-text",
            value if value == "application/pdf" => "bi-file-earmark-pdf",
            value if value.starts_with("image/") => "bi-file-earmark-image",
            value if value.starts_with("audio/") => "bi-file-earmark-music",
            value if value.starts_with("video/") => "bi-file-earmark-play",
            value if value.contains("zip") || value.contains("compressed") => "bi-file-earmark-zip",
            _ => "bi-file-earmark",
        }
    }
}

fn encode_query_value(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn explorer_url(path: &str, search: &str, tag: &str, sort: &str, direction: &str) -> String {
    format!(
        "/my-drive?path={}&q={}&tag={}&sort={}&direction={}",
        encode_query_value(path),
        encode_query_value(search),
        encode_query_value(tag),
        encode_query_value(sort),
        encode_query_value(direction),
    )
}

fn sort_url(
    path: &str,
    search: &str,
    tag: &str,
    current_sort: &str,
    descending: bool,
    requested_sort: &str,
) -> String {
    let next_direction = if current_sort == requested_sort && !descending {
        "desc"
    } else {
        "asc"
    };
    explorer_url(path, search, tag, requested_sort, next_direction)
}

async fn apply_my_drive_tag(
    State(state): State<Arc<AppState>>,
    Form(form): Form<ApplyTagForm>,
) -> Result<Redirect, StatusCode> {
    let database = state.database().map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let selected_items: Vec<String> = form.selected_item_ids
        .split(',')
        .map(str::trim)
        .filter(|item_id| !item_id.is_empty())
        .map(str::to_string)
        .collect();
    let applied = database::inventory::apply_tag_recursively(
        &database,
        &selected_items,
        &form.tag,
    ).map_err(|error| {
        eprintln!("Unable to apply My Drive tag: {error}");
        StatusCode::BAD_REQUEST
    })?;
    println!(
        "My Drive tag applied: tag={}, selected_items={}, applied_items={applied}",
        form.tag,
        selected_items.len(),
    );
    let mut url = explorer_url(
        &form.path,
        &form.q,
        &form.tag_filter,
        &form.sort,
        &form.direction,
    );
    url.push_str(&format!("&tagged={applied}"));
    Ok(Redirect::to(&url))
}

async fn tags_page(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SettingsQuery>,
) -> Result<Html<String>, StatusCode> {
    let database = state.database().map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let tags = database::inventory::list_tags(&database)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let rclone_state = state.rclone_state();
    let google_client_state = state.google_client_state();
    let google_remotes_state = state.google_remotes_state();
    let metadata_state = state.metadata_state();
    render_template(&TagsTemplate {
        title: "Tags - BOREAL",
        active_page: "tags",
        alerts: build_alerts(&rclone_state, &google_client_state),
        status_items: build_status_items(
            &rclone_state, &google_client_state, &google_remotes_state,
            &metadata_state, configured_remote_count(&state.runtime, &rclone_state),
        ),
        poll_rclone: should_poll_ui(&rclone_state, &google_remotes_state, &metadata_state),
        tags,
        saved: query.saved,
    })
}

async fn create_tag(
    State(state): State<Arc<AppState>>,
    Form(form): Form<TagForm>,
) -> Result<Redirect, StatusCode> {
    let database = state.database().map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    database::inventory::create_tag(&database, &form.name, &form.color)
        .map_err(|error| {
            eprintln!("Unable to create tag: {error}");
            StatusCode::BAD_REQUEST
        })?;
    println!("Tag created: name={}", form.name.trim());
    Ok(Redirect::to("/tags?saved=true"))
}

async fn update_tag(
    State(state): State<Arc<AppState>>,
    Form(form): Form<TagForm>,
) -> Result<Redirect, StatusCode> {
    let database = state.database().map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    database::inventory::update_tag(&database, &form.slug, &form.name, &form.color)
        .map_err(|error| {
            eprintln!("Unable to update tag: {error}");
            StatusCode::BAD_REQUEST
        })?;
    println!("Tag updated: slug={}", form.slug);
    Ok(Redirect::to("/tags?saved=true"))
}

fn tag_text_color(color: &str) -> &'static str {
    let value = u32::from_str_radix(color.trim_start_matches('#'), 16).unwrap_or(0x6c757d);
    let red = (value >> 16) & 0xff;
    let green = (value >> 8) & 0xff;
    let blue = value & 0xff;
    if red * 299 + green * 587 + blue * 114 > 150_000 { "#212529" } else { "#ffffff" }
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
    let metadata_state =
        state.metadata_state();

    let template = StatusTemplate {
        status_items: build_status_items(
            &rclone_state,
            &google_client_state,
            &google_remotes_state,
            &metadata_state,
            configured_remote_count(&state.runtime, &rclone_state),
        ),

        poll_rclone: should_poll_ui(
            &rclone_state,
            &google_remotes_state,
            &metadata_state,
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

async fn ui_drive_summaries(
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, StatusCode> {
    let metadata_state = state.metadata_state();
    let template = DriveSummariesTemplate {
        metadata: build_metadata_view(
            &metadata_state,
            true,
            false,
        ),
    };
    render_template(&template)
}

async fn ui_metadata_progress(
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, StatusCode> {
    let remotes = state.google_remotes_state();
    let rclone_state = state.rclone_state();
    let available = matches!(
        remotes.rw,
        RemoteState::Ready
    ) && matches!(
        remotes.ro,
        RemoteState::Ready
    );
    let metadata_state = state.metadata_state();

    render_template(
        &MetadataProgressTemplate {
            metadata: build_metadata_view(
                &metadata_state,
                available,
                should_poll_setup(
                    &rclone_state,
                    &remotes,
                ),
            ),
        },
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

async fn start_metadata_update(
    State(state): State<Arc<AppState>>,
) -> Result<Redirect, StatusCode> {
    let remotes = state.google_remotes_state();

    if !matches!(remotes.ro, RemoteState::Ready) {
        return Err(
            StatusCode::PRECONDITION_FAILED,
        );
    }

    AppState::start_metadata_update(
        state,
    )
    .map_err(
        |error| {
            eprintln!(
                "Unable to start metadata update: {error}"
            );
            StatusCode::CONFLICT
        },
    )?;

    Ok(
        Redirect::to(
            "/",
        ),
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

fn should_poll_setup(
    rclone_state: &RcloneState,
    remotes_state: &GoogleRemotesState,
) -> bool {
    should_poll_rclone(rclone_state)
        || matches!(remotes_state.rw, RemoteState::Configuring)
        || matches!(remotes_state.ro, RemoteState::Configuring)
}

fn should_poll_ui(
    rclone_state: &RcloneState,
    remotes_state: &GoogleRemotesState,
    metadata_state: &MetadataState,
) -> bool {
    should_poll_setup(
        rclone_state,
        remotes_state,
    ) || matches!(
        metadata_state,
        MetadataState::Updating(_)
    )
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
    _google_remotes_state: &GoogleRemotesState,
    metadata_state: &MetadataState,
    configured_remote_count: usize,
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

    let (remote_value, remote_class) = if configured_remote_count == 0 {
        ("0 configured".to_string(), "text-warning")
    } else {
        (format!("{configured_remote_count} configured"), "text-success")
    };

    let (
        metadata_value,
        metadata_class,
        metadata_spinner,
    ) = match metadata_state {
        MetadataState::NotSynchronized => (
            "Not synchronized".to_string(),
            "text-warning",
            false,
        ),
        MetadataState::Updating(
            progress,
        ) => (
            progress.phase.to_string(),
            "text-primary",
            true,
        ),
        MetadataState::Synchronized(
            _,
        ) => (
            "00:00:00".to_string(),
            "boreal-metadata-age text-success",
            false,
        ),
        MetadataState::Error(
            _,
        ) => (
            "Update failed".to_string(),
            "text-danger",
            false,
        ),
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
            spinner: false,
            age_timestamp: String::new(),
        },

        StatusItem {
            icon: "bi-key",
            label: "ClientID",
            value: client_id_value,
            value_class: client_id_value_class,
            value_url: String::new(),
            spinner: false,
            age_timestamp: String::new(),
        },

        StatusItem {
            icon: "bi-cloud",
            label: "Remotes",
            value: remote_value,
            value_class: remote_class,
            value_url: String::new(),
            spinner: false,
            age_timestamp: String::new(),
        },

        StatusItem {
            icon: "bi-database",
            label: "Metadata",
            value: metadata_value,
            value_class: metadata_class,
            value_url: String::new(),
            spinner: metadata_spinner,
            age_timestamp: match metadata_state {
                MetadataState::Synchronized(summary) => summary.completed_at.clone(),
                _ => String::new(),
            },
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
            spinner: false,
            age_timestamp: String::new(),
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
