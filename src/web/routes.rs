use std::sync::Arc;

use askama::Template;

use axum::{
    Router,
    extract::{Form, Multipart, Path, Query, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};

use crate::{
    app::{AppState, GoogleClientState, GoogleRemotesState, MetadataState, RcloneState},
    database::{
        self,
        settings::{self, InventorySettings},
    },
    google,
    rclone::{
        self,
        remotes::{RemoteKind, RemoteState},
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
    pub shared_indexed: bool,
    pub shared_files_scanned: u64,
    pub shared_folders_scanned: u64,
    pub shared_permissions_scanned: u64,
    pub shared_size_label: String,
    pub shared_completed_at: String,
}

#[allow(dead_code)]
#[derive(Template)]
#[template(path = "dashboard.html", config = "askama.toml")]
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
#[template(path = "settings.html", config = "askama.toml")]
struct SettingsTemplate {
    title: &'static str,
    active_page: &'static str,
    alerts: Vec<AlertItem>,
    status_items: Vec<StatusItem>,
    poll_rclone: bool,
    settings: InventorySettings,
    saved: bool,
    error: String,
    notice: String,
    directory_source: database::directory::LinkedSheetStatus,
}

#[allow(dead_code)]
#[derive(Template)]
#[template(path = "about.html", config = "askama.toml")]
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
#[template(path = "remotes.html", config = "askama.toml")]
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
    pub is_deleted: bool,
}

#[allow(dead_code)]
pub struct TagPill {
    pub name: String,
    pub color: String,
    pub text_color: &'static str,
}

#[allow(dead_code)]
#[derive(Template)]
#[template(path = "my-drive.html", config = "askama.toml")]
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
    type_filter: String,
    size_filter: String,
    modified_filter: String,
    owner_filter: String,
    permission_filter: String,
    include_deleted: bool,
    heading: &'static str,
    description: &'static str,
    root_label: &'static str,
    explorer_path: &'static str,
    tag_action: &'static str,
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
#[template(path = "directory.html", config = "askama.toml")]
struct DirectoryTemplate {
    title: &'static str,
    active_page: &'static str,
    alerts: Vec<AlertItem>,
    status_items: Vec<StatusItem>,
    poll_rclone: bool,
    summary: database::directory::DirectorySummary,
    principals: Vec<database::directory::PrincipalRow>,
    organizations: Vec<database::directory::OrganizationRow>,
    remote_accounts: Vec<database::directory::RemoteAccountRow>,
    import_complete: bool,
    imported_created: u64,
    imported_updated: u64,
    imported_rejected: u64,
    name_filter: String,
    email_filter: String,
    type_filter: String,
    status_filter: String,
    departure_filter: String,
    organization_filter: String,
}

#[allow(dead_code)]
#[derive(Template)]
#[template(path = "principal.html", config = "askama.toml")]
struct PrincipalTemplate {
    title: String,
    active_page: &'static str,
    alerts: Vec<AlertItem>,
    status_items: Vec<StatusItem>,
    poll_rclone: bool,
    principal: database::directory::PrincipalRow,
    associations: Vec<database::directory::PrincipalAssociationRow>,
}

#[allow(dead_code)]
#[derive(Template)]
#[template(path = "partials/alerts.html", config = "askama.toml")]
struct AlertsTemplate {
    alerts: Vec<AlertItem>,
    poll_rclone: bool,
}

#[allow(dead_code)]
#[derive(Template)]
#[template(path = "partials/status.html", config = "askama.toml")]
struct StatusTemplate {
    status_items: Vec<StatusItem>,
    poll_rclone: bool,
}

#[allow(dead_code)]
#[derive(Template)]
#[template(path = "partials/setup-progress.html", config = "askama.toml")]
struct SetupProgressTemplate {
    setup_steps: Vec<SetupStep>,
    setup_percent: u8,
    poll_rclone: bool,
}

#[allow(dead_code)]
#[derive(Template)]
#[template(path = "partials/metadata-progress.html", config = "askama.toml")]
struct MetadataProgressTemplate {
    metadata: MetadataView,
}

#[allow(dead_code)]
#[derive(Template)]
#[template(path = "partials/metadata-update-modal-content.html", config = "askama.toml")]
struct MetadataUpdateModalTemplate {
    metadata: MetadataView,
    progress_percent: u8,
    timing_available: bool,
    elapsed_label: String,
    estimated_total_label: String,
    remaining_label: String,
    timing_samples: u64,
}

#[allow(dead_code)]
#[derive(Template)]
#[template(path = "partials/drive-summaries.html", config = "askama.toml")]
struct DriveSummariesTemplate {
    metadata: MetadataView,
}

#[derive(serde::Deserialize)]
struct SettingsQuery {
    #[serde(default)]
    saved: bool,
}

#[derive(serde::Deserialize, Default)]
struct DirectoryQuery {
    #[serde(default)]
    imported: bool,
    #[serde(default)]
    created: u64,
    #[serde(default)]
    updated: u64,
    #[serde(default)]
    rejected: u64,
    #[serde(default)]
    name_filter: String,
    #[serde(default)]
    email_filter: String,
    #[serde(default)]
    type_filter: String,
    #[serde(default)]
    status_filter: String,
    #[serde(default)]
    departure_filter: String,
    #[serde(default)]
    organization_filter: String,
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
    #[serde(default)]
    type_filter: String,
    #[serde(default)]
    size_filter: String,
    #[serde(default)]
    modified_filter: String,
    #[serde(default)]
    owner_filter: String,
    #[serde(default)]
    permission_filter: String,
    #[serde(default)]
    include_deleted: bool,
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
    type_filter: String,
    size_filter: String,
    modified_filter: String,
    owner_filter: String,
    permission_filter: String,
    #[serde(default)]
    include_deleted: bool,
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
    #[serde(default)]
    directory_sheet_enabled: Option<String>,
    #[serde(default)]
    directory_sheet_url: String,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(index))
        .route("/about", get(about))
        .route("/assets/uaf-logo.png", get(uaf_logo))
        .route("/assets/acep-logo.png", get(acep_logo))
        .route("/remotes", get(remotes_page))
        .route("/my-drive", get(my_drive_page))
        .route("/my-drive/tags", post(apply_my_drive_tag))
        .route("/shared-with-me", get(shared_with_me_page))
        .route("/shared-with-me/tags", post(apply_shared_with_me_tag))
        .route("/tags", get(tags_page))
        .route("/directory", get(directory_page))
        .route("/directory/principals/{principal_id}", get(principal_page))
        .route("/directory/import/csv", post(import_directory_csv))
        .route("/tags/create", post(create_tag))
        .route("/tags/update", post(update_tag))
        .route("/settings", get(settings_page).post(save_settings))
        .route("/settings/directory/test", post(test_directory_sheet))
        .route("/status", get(status))
        .route("/rclone-gui", get(open_rclone_gui))
        .route("/ui/alerts", get(ui_alerts))
        .route("/ui/status", get(ui_status))
        .route("/ui/setup-progress", get(ui_setup_progress))
        .route("/ui/metadata-progress", get(ui_metadata_progress))
        .route("/ui/metadata-update-modal", get(ui_metadata_update_modal))
        .route("/ui/drive-summaries", get(ui_drive_summaries))
        .route("/setup/google-client/import", post(import_google_client))
        .route("/setup/remotes/my-drive-ro", post(setup_my_drive_ro))
        .route("/metadata/update", post(start_metadata_update))
        .route("/app/quit", post(quit))
}

async fn uaf_logo() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        include_bytes!("../../tmpl/html/img/UAFLogo_A_blue.png").as_slice(),
    )
}

async fn acep_logo() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        include_bytes!("../../tmpl/html/img/ACEP Logo.png").as_slice(),
    )
}

async fn index(State(state): State<Arc<AppState>>) -> Result<Html<String>, StatusCode> {
    let rclone_state = state.rclone_state();

    let google_client_state = state.google_client_state();
    let google_remotes_state = state.google_remotes_state();
    let metadata_state = state.metadata_state();

    let alerts = build_alerts(&rclone_state, &google_client_state);

    let status_items = build_status_items(
        &rclone_state,
        &google_client_state,
        &google_remotes_state,
        &metadata_state,
        configured_remote_count(&state.runtime, &rclone_state),
        authenticated_google_email(&state),
    );

    let (setup_steps, setup_percent) =
        build_setup_progress(&rclone_state, &google_client_state, &google_remotes_state);

    let poll_rclone = should_poll_ui(&rclone_state, &google_remotes_state, &metadata_state);

    let shared_summary = latest_shared_summary(&state);
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
            should_poll_setup(&rclone_state, &google_remotes_state),
            shared_summary.as_ref(),
        ),
    };

    render_template(&template)
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

        RcloneState::Ready(status) => SetupStep {
            icon: "bi-check-circle-fill",
            title: "Install Rclone",
            description: format!("{} is installed and ready.", status.version),
            state_label: "Complete",
            state_class: "text-bg-success",
            complete: true,
            modal_target: "",
            remote_actions: Vec::new(),
        },

        RcloneState::Error(error) => SetupStep {
            icon: "bi-exclamation-triangle-fill",
            title: "Install Rclone",
            description: format!("Rclone setup failed: {error}"),
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

    let remote_complete = matches!(google_remotes_state.ro, RemoteState::Ready);
    let remote_busy = matches!(google_remotes_state.ro, RemoteState::Configuring);
    let prerequisites_ready = matches!(rclone_state, RcloneState::Ready(_))
        && matches!(google_client_state, GoogleClientState::Ready(_));

    let remote_step = SetupStep {
        icon: if remote_complete { "bi-check-circle-fill" } else { "bi-cloud-plus" },
        title: "Configure My Drive Read-Only Remote",
        description:
            "Authorize a read-only Google Drive connection for inventory and exploration. Google opens a browser tab for authorization."
                .to_string(),
        state_label: if remote_complete { "Complete" } else { "Set up" },
        state_class: if remote_complete { "text-bg-success" } else { "text-bg-warning" },
        complete: remote_complete,
        modal_target: "",
        remote_actions: vec![
            build_remote_action(
                "Setup My Drive RO",
                "/setup/remotes/my-drive-ro",
                &google_remotes_state.ro,
                prerequisites_ready,
                remote_busy,
            ),
        ],
    };

    let steps = vec![rclone_step, google_step, remote_step];

    let complete_count = steps.iter().filter(|step| step.complete).count();

    let setup_percent = (complete_count * 100 / steps.len()) as u8;

    (steps, setup_percent)
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
            RemoteState::Configuring => {
                "Complete the Google authorization in the browser tab opened by Rclone.".to_string()
            }
            _ => String::new(),
        },
    }
}

fn build_metadata_view(
    state: &MetadataState,
    available: bool,
    poll_for_setup: bool,
    shared_summary: Option<&database::inventory::InventorySummary>,
) -> MetadataView {
    let shared_indexed = shared_summary.is_some();
    let shared = shared_summary.cloned().unwrap_or_default();
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
            shared_indexed,
            shared_files_scanned: shared.files_scanned,
            shared_folders_scanned: shared.folders_scanned,
            shared_permissions_scanned: shared.permissions_scanned,
            shared_size_label: format_bytes(shared.bytes_discovered),
            shared_completed_at: shared.completed_at.clone(),
        },

        MetadataState::Updating(progress) => MetadataView {
            available,
            poll: true,
            updating: true,
            state_label: "Updating".to_string(),
            state_class: "text-bg-primary",
            phase: progress.phase.to_string(),
            files_scanned: progress.files_scanned,
            folders_scanned: progress.folders_scanned,
            permissions_scanned: progress.permissions_scanned,
            size_label: format_bytes(progress.bytes_discovered),
            errors: progress.errors,
            completed_at: String::new(),
            shared_indexed,
            shared_files_scanned: shared.files_scanned,
            shared_folders_scanned: shared.folders_scanned,
            shared_permissions_scanned: shared.permissions_scanned,
            shared_size_label: format_bytes(shared.bytes_discovered),
            shared_completed_at: shared.completed_at.clone(),
        },

        MetadataState::Synchronized(summary) => MetadataView {
            available,
            poll: poll_for_setup,
            updating: false,
            state_label: "Synchronized".to_string(),
            state_class: "text-bg-success",
            phase: "My Drive inventory is current as of the completed update.".to_string(),
            files_scanned: summary.files_scanned,
            folders_scanned: summary.folders_scanned,
            permissions_scanned: summary.permissions_scanned,
            size_label: format_bytes(summary.bytes_discovered),
            errors: 0,
            completed_at: summary.completed_at.clone(),
            shared_indexed,
            shared_files_scanned: shared.files_scanned,
            shared_folders_scanned: shared.folders_scanned,
            shared_permissions_scanned: shared.permissions_scanned,
            shared_size_label: format_bytes(shared.bytes_discovered),
            shared_completed_at: shared.completed_at.clone(),
        },

        MetadataState::Error(error) => MetadataView {
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
            shared_indexed,
            shared_files_scanned: shared.files_scanned,
            shared_folders_scanned: shared.folders_scanned,
            shared_permissions_scanned: shared.permissions_scanned,
            shared_size_label: format_bytes(shared.bytes_discovered),
            shared_completed_at: shared.completed_at.clone(),
        },
    }
}

fn format_bytes(bytes: u64) -> String {
    const GB: f64 = 1_000_000_000.0;
    const MB: f64 = 1_000_000.0;
    const KB: f64 = 1_000.0;

    if bytes as f64 >= GB {
        format!("{:.1} GB", bytes as f64 / GB,)
    } else if bytes as f64 >= MB {
        format!("{:.1} MB", bytes as f64 / MB,)
    } else if bytes as f64 >= KB {
        format!("{:.1} kB", bytes as f64 / KB,)
    } else {
        format!("{bytes} B",)
    }
}

async fn settings_page(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SettingsQuery>,
) -> Result<Html<String>, StatusCode> {
    let database = state.database().map_err(|error| {
        eprintln!("Unable to open settings: {error}");
        StatusCode::SERVICE_UNAVAILABLE
    })?;
    let inventory_settings = settings::load(&database).map_err(|error| {
        eprintln!("Unable to load settings: {error}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    render_settings(&state, inventory_settings, query.saved, String::new(), String::new())
}

async fn save_settings(
    State(state): State<Arc<AppState>>,
    Form(form): Form<SettingsForm>,
) -> Result<axum::response::Response, StatusCode> {
    let inventory_settings = InventorySettings {
        automatic_updates: form.automatic_updates.is_some(),
        refresh_interval_hours: form.refresh_interval_hours,
        full_reconciliation_days: form.full_reconciliation_days,
        update_when_overdue_at_startup: form.update_when_overdue_at_startup.is_some(),
        permission_scanning: form.permission_scanning.is_some(),
        directory_sheet_enabled: form.directory_sheet_enabled.is_some(),
        directory_sheet_url: form.directory_sheet_url.trim().to_string(),
    };
    if inventory_settings.directory_sheet_enabled {
        if let Err(error) = crate::rclone::identity::parse_google_sheet_url(
            &inventory_settings.directory_sheet_url,
        ) {
            return render_settings(&state, inventory_settings, false, error.to_string(), String::new())
                .map(axum::response::IntoResponse::into_response);
        }
    }
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    match settings::save(&database, &inventory_settings) {
        Ok(()) => Ok(Redirect::to("/settings?saved=true").into_response()),

        Err(error) => render_settings(&state, inventory_settings, false, error.to_string(), String::new())
            .map(axum::response::IntoResponse::into_response),
    }
}

async fn test_directory_sheet(
    State(state): State<Arc<AppState>>,
    Form(form): Form<SettingsForm>,
) -> Result<axum::response::Response, StatusCode> {
    let inventory_settings = InventorySettings {
        automatic_updates: form.automatic_updates.is_some(),
        refresh_interval_hours: form.refresh_interval_hours,
        full_reconciliation_days: form.full_reconciliation_days,
        update_when_overdue_at_startup: form.update_when_overdue_at_startup.is_some(),
        permission_scanning: form.permission_scanning.is_some(),
        directory_sheet_enabled: form.directory_sheet_enabled.is_some(),
        directory_sheet_url: form.directory_sheet_url.trim().to_string(),
    };
    if let Err(error) = crate::rclone::identity::parse_google_sheet_url(
        &inventory_settings.directory_sheet_url,
    ) {
        return render_settings(&state, inventory_settings, false, error.to_string(), String::new())
            .map(axum::response::IntoResponse::into_response);
    }
    let database = state.database().map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if let Err(error) = settings::save(&database, &inventory_settings) {
        return render_settings(&state, inventory_settings, false, error.to_string(), String::new())
            .map(axum::response::IntoResponse::into_response);
    }
    let worker_state = Arc::clone(&state);
    let url = inventory_settings.directory_sheet_url.clone();
    let rclone_path = match state.rclone_state() {
        RcloneState::Ready(status) => status.path,
        _ => {
            return render_settings(
                &state,
                inventory_settings,
                false,
                "Rclone must be ready before directory access can be tested".to_string(),
                String::new(),
            )
            .map(axum::response::IntoResponse::into_response)
        }
    };
    let result: Result<(), String> = tokio::task::spawn_blocking(move || {
        crate::rclone::identity::fetch_read_only_account(&worker_state.runtime, &rclone_path)
            .map_err(|error| error.to_string())?;
        let (_, csv) = crate::rclone::identity::download_google_sheet_csv(&worker_state.runtime, &url)
            .map_err(|error| error.to_string())?;
        database::directory::validate_csv(&csv).map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    match result {
        Ok(()) => render_settings(
            &state,
            inventory_settings,
            false,
            String::new(),
            "Directory spreadsheet access verified; the selected worksheet contains a usable email column.".to_string(),
        ),
        Err(error) => render_settings(
            &state,
            inventory_settings,
            false,
            error.to_string(),
            String::new(),
        ),
    }
    .map(axum::response::IntoResponse::into_response)
}

fn render_settings(
    state: &AppState,
    inventory_settings: InventorySettings,
    saved: bool,
    error: String,
    notice: String,
) -> Result<Html<String>, StatusCode> {
    let rclone_state = state.rclone_state();
    let google_client_state = state.google_client_state();
    let google_remotes_state = state.google_remotes_state();
    let metadata_state = state.metadata_state();
    let directory_source = state
        .database()
        .ok()
        .and_then(|database| database::directory::linked_sheet_status(&database).ok())
        .unwrap_or_default();

    let template = SettingsTemplate {
        title: "Settings - BOREAL",
        active_page: "settings",
        alerts: build_alerts(&rclone_state, &google_client_state),
        status_items: build_status_items(
            &rclone_state,
            &google_client_state,
            &google_remotes_state,
            &metadata_state,
            configured_remote_count(&state.runtime, &rclone_state),
            authenticated_google_email(&state),
        ),
        poll_rclone: should_poll_ui(&rclone_state, &google_remotes_state, &metadata_state),
        settings: inventory_settings,
        saved,
        error,
        notice,
        directory_source,
    };

    render_template(&template)
}

async fn about(State(state): State<Arc<AppState>>) -> Result<Html<String>, StatusCode> {
    let rclone_state = state.rclone_state();

    let google_client_state = state.google_client_state();
    let google_remotes_state = state.google_remotes_state();
    let metadata_state = state.metadata_state();

    let alerts = build_alerts(&rclone_state, &google_client_state);

    let status_items = build_status_items(
        &rclone_state,
        &google_client_state,
        &google_remotes_state,
        &metadata_state,
        configured_remote_count(&state.runtime, &rclone_state),
        authenticated_google_email(&state),
    );

    let poll_rclone = should_poll_ui(&rclone_state, &google_remotes_state, &metadata_state);

    let template = AboutTemplate {
        title: "About BOREAL",
        active_page: "about",
        alerts,
        status_items,
        poll_rclone,
    };

    render_template(&template)
}

async fn status() -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn remotes_page(State(state): State<Arc<AppState>>) -> Result<Html<String>, StatusCode> {
    let rclone_state = state.rclone_state();
    let google_client_state = state.google_client_state();
    let google_remotes_state = state.google_remotes_state();
    let metadata_state = state.metadata_state();

    let listed = match &rclone_state {
        RcloneState::Ready(status) => {
            rclone::remotes::list_configured(&state.runtime, &status.path)
        }
        _ => Err("Rclone is not ready".into()),
    };
    let (remotes, error) = match listed {
        Ok(remotes) => (
            remotes
                .into_iter()
                .map(|remote| {
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
                })
                .collect(),
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
            authenticated_google_email(&state),
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
    render_drive_explorer(
        &state,
        query,
        database::inventory::MY_DRIVE_SCOPE,
        "my-drive",
        "My Drive Explorer",
        "Browse the latest local My Drive metadata inventory and open items in Google Drive.",
        "My Drive",
        "/my-drive",
        "/my-drive/tags",
    )
}

async fn shared_with_me_page(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DrivePathQuery>,
) -> Result<Html<String>, StatusCode> {
    render_drive_explorer(
        &state,
        query,
        database::inventory::SHARED_WITH_ME_SCOPE,
        "shared-with-me",
        "Shared with me Explorer",
        "Browse content other people have shared with the authenticated Google account.",
        "Shared with me",
        "/shared-with-me",
        "/shared-with-me/tags",
    )
}

fn render_drive_explorer(
    state: &AppState,
    query: DrivePathQuery,
    inventory_scope: &'static str,
    active_page: &'static str,
    heading: &'static str,
    description: &'static str,
    root_label: &'static str,
    explorer_path: &'static str,
    tag_action: &'static str,
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
    let (exclude_owner, owner_filter) = match query.owner_filter.strip_prefix('!') {
        Some(owner) => (true, owner.trim()),
        None => (false, query.owner_filter.trim()),
    };
    let (items, error) = match database::inventory::list_drive_directory(
        &database,
        inventory_scope,
        parent_filter,
        &query.q,
        &query.tag,
        &query.type_filter,
        &query.size_filter,
        &query.modified_filter,
        owner_filter,
        exclude_owner,
        &query.permission_filter,
        query.include_deleted,
        sort,
        descending,
    ) {
        Ok(items) => (items, String::new()),
        Err(error) => {
            eprintln!("Unable to list My Drive explorer directory: {error}");
            (Vec::new(), error.to_string())
        }
    };
    let rows = items
        .into_iter()
        .map(|item| DriveExplorerRow {
            drive_url: if item.is_directory {
                format!("https://drive.google.com/drive/folders/{}", item.item_id)
            } else {
                format!("https://drive.google.com/open?id={}", item.item_id)
            },
            item_id: item.item_id.clone(),
            name: item.name,
            name_url: if item.is_directory {
                explorer_url(
                    explorer_path,
                    &item.relative_path,
                    "",
                    &query.tag,
                    &query.type_filter,
                    &query.size_filter,
                    &query.modified_filter,
                    &query.owner_filter,
                    &query.permission_filter,
                    sort,
                    if descending { "desc" } else { "asc" },
                    query.include_deleted,
                )
            } else {
                format!("https://drive.google.com/open?id={}", item.item_id)
            },
            name_new_tab: !item.is_directory,
            is_directory: item.is_directory,
            type_icon: mime_icon(item.is_directory, item.mime_type.as_deref()),
            mime_type: if item.is_directory {
                "Folder".to_string()
            } else {
                item.mime_type
                    .unwrap_or_else(|| "Unknown file type".to_string())
            },
            tags: item
                .tags
                .into_iter()
                .map(|tag| TagPill {
                    text_color: tag_text_color(&tag.color),
                    name: tag.name,
                    color: tag.color,
                })
                .collect(),
            permissions: if item.permissions.is_empty() {
                "—".to_string()
            } else {
                item.permissions.join(", ")
            },
            size: item
                .size_bytes
                .map(format_bytes)
                .unwrap_or_else(|| "—".to_string()),
            modified_at: item.modified_at.unwrap_or_else(|| "—".to_string()),
            owner_email: item.owner_email.unwrap_or_else(|| "—".to_string()),
            is_deleted: item.is_deleted,
        })
        .collect();
    let parent_path = query
        .path
        .rsplit_once('/')
        .map(|(parent, _)| parent.to_string())
        .unwrap_or_default();
    let tags = database::inventory::list_tags(&database).map_err(|error| {
        eprintln!("Unable to load My Drive tags: {error}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let template = MyDriveTemplate {
        title: heading,
        active_page,
        alerts: build_alerts(&rclone_state, &google_client_state),
        status_items: build_status_items(
            &rclone_state,
            &google_client_state,
            &google_remotes_state,
            &metadata_state,
            configured_remote_count(&state.runtime, &rclone_state),
            authenticated_google_email(&state),
        ),
        poll_rclone: should_poll_ui(&rclone_state, &google_remotes_state, &metadata_state),
        current_path: if query.path.is_empty() {
            root_label.to_string()
        } else {
            query.path.clone()
        },
        parent_path,
        has_parent,
        rows,
        error,
        search: query.q.clone(),
        sort: sort.to_string(),
        direction: if descending {
            "desc".to_string()
        } else {
            "asc".to_string()
        },
        name_sort_url: sort_url(explorer_path, &query, sort, descending, "name"),
        type_sort_url: sort_url(explorer_path, &query, sort, descending, "type"),
        size_sort_url: sort_url(explorer_path, &query, sort, descending, "size"),
        modified_sort_url: sort_url(explorer_path, &query, sort, descending, "modified"),
        owner_sort_url: sort_url(explorer_path, &query, sort, descending, "owner"),
        clear_search_url: explorer_url(
            explorer_path,
            &query.path,
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            sort,
            if descending { "desc" } else { "asc" },
            false,
        ),
        tags,
        tag_filter: query.tag,
        tagged_count: query.tagged,
        type_filter: query.type_filter,
        size_filter: query.size_filter,
        modified_filter: query.modified_filter,
        owner_filter: query.owner_filter,
        permission_filter: query.permission_filter,
        include_deleted: query.include_deleted,
        heading,
        description,
        root_label,
        explorer_path,
        tag_action,
    };
    render_template(&template)
}

fn mime_icon(is_directory: bool, mime_type: Option<&str>) -> &'static str {
    if is_directory {
        "bi-folder-fill"
    } else {
        match mime_type.unwrap_or("") {
            value if value.contains("spreadsheet") || value.contains("excel") => {
                "bi-file-earmark-spreadsheet"
            }
            value if value.contains("presentation") || value.contains("powerpoint") => {
                "bi-file-earmark-slides"
            }
            value
                if value.contains("document")
                    || value.contains("word")
                    || value.starts_with("text/") =>
            {
                "bi-file-earmark-text"
            }
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

fn explorer_url(
    explorer_path: &str,
    path: &str,
    search: &str,
    tag: &str,
    type_filter: &str,
    size_filter: &str,
    modified_filter: &str,
    owner_filter: &str,
    permission_filter: &str,
    sort: &str,
    direction: &str,
    include_deleted: bool,
) -> String {
    format!(
        "{explorer_path}?path={}&q={}&tag={}&type_filter={}&size_filter={}&modified_filter={}&owner_filter={}&permission_filter={}&sort={}&direction={}&include_deleted={include_deleted}",
        encode_query_value(path),
        encode_query_value(search),
        encode_query_value(tag),
        encode_query_value(type_filter),
        encode_query_value(size_filter),
        encode_query_value(modified_filter),
        encode_query_value(owner_filter),
        encode_query_value(permission_filter),
        encode_query_value(sort),
        encode_query_value(direction),
    )
}

fn sort_url(
    explorer_path: &str,
    query: &DrivePathQuery,
    current_sort: &str,
    descending: bool,
    requested_sort: &str,
) -> String {
    let next_direction = if current_sort == requested_sort && !descending {
        "desc"
    } else {
        "asc"
    };
    explorer_url(
        explorer_path,
        &query.path,
        &query.q,
        &query.tag,
        &query.type_filter,
        &query.size_filter,
        &query.modified_filter,
        &query.owner_filter,
        &query.permission_filter,
        requested_sort,
        next_direction,
        query.include_deleted,
    )
}

async fn apply_my_drive_tag(
    State(state): State<Arc<AppState>>,
    Form(form): Form<ApplyTagForm>,
) -> Result<Redirect, StatusCode> {
    apply_drive_tag(
        &state,
        form,
        database::inventory::MY_DRIVE_SCOPE,
        "/my-drive",
    )
}

async fn apply_shared_with_me_tag(
    State(state): State<Arc<AppState>>,
    Form(form): Form<ApplyTagForm>,
) -> Result<Redirect, StatusCode> {
    apply_drive_tag(
        &state,
        form,
        database::inventory::SHARED_WITH_ME_SCOPE,
        "/shared-with-me",
    )
}

fn apply_drive_tag(
    state: &AppState,
    form: ApplyTagForm,
    inventory_scope: &str,
    explorer_path: &str,
) -> Result<Redirect, StatusCode> {
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let selected_items: Vec<String> = form
        .selected_item_ids
        .split(',')
        .map(str::trim)
        .filter(|item_id| !item_id.is_empty())
        .map(str::to_string)
        .collect();
    let applied = database::inventory::apply_tag_recursively_for_scope(
        &database,
        inventory_scope,
        &selected_items,
        &form.tag,
    )
    .map_err(|error| {
        eprintln!("Unable to apply My Drive tag: {error}");
        StatusCode::BAD_REQUEST
    })?;
    println!(
        "My Drive tag applied: tag={}, selected_items={}, applied_items={applied}",
        form.tag,
        selected_items.len(),
    );
    let mut url = explorer_url(
        explorer_path,
        &form.path,
        &form.q,
        &form.tag_filter,
        &form.type_filter,
        &form.size_filter,
        &form.modified_filter,
        &form.owner_filter,
        &form.permission_filter,
        &form.sort,
        &form.direction,
        form.include_deleted,
    );
    url.push_str(&format!("&tagged={applied}"));
    Ok(Redirect::to(&url))
}

async fn tags_page(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SettingsQuery>,
) -> Result<Html<String>, StatusCode> {
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let tags =
        database::inventory::list_tags(&database).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let rclone_state = state.rclone_state();
    let google_client_state = state.google_client_state();
    let google_remotes_state = state.google_remotes_state();
    let metadata_state = state.metadata_state();
    render_template(&TagsTemplate {
        title: "Tags - BOREAL",
        active_page: "tags",
        alerts: build_alerts(&rclone_state, &google_client_state),
        status_items: build_status_items(
            &rclone_state,
            &google_client_state,
            &google_remotes_state,
            &metadata_state,
            configured_remote_count(&state.runtime, &rclone_state),
            authenticated_google_email(&state),
        ),
        poll_rclone: should_poll_ui(&rclone_state, &google_remotes_state, &metadata_state),
        tags,
        saved: query.saved,
    })
}

async fn directory_page(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DirectoryQuery>,
) -> Result<Html<String>, StatusCode> {
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let summary = database::directory::summary(&database).map_err(|error| {
        log::error!("Unable to load directory summary: {error}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let principals = database::directory::list_principals_filtered(
        &database,
        &query.name_filter,
        &query.email_filter,
        &query.type_filter,
        &query.status_filter,
        &query.departure_filter,
        &query.organization_filter,
    )
    .map_err(|error| {
        log::error!("Unable to load directory principals: {error}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let organizations = database::directory::list_organizations(&database).map_err(|error| {
        log::error!("Unable to load directory organizations: {error}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let remote_accounts =
        database::directory::list_remote_accounts(&database).map_err(|error| {
            log::error!("Unable to load authenticated accounts: {error}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let rclone_state = state.rclone_state();
    let google_client_state = state.google_client_state();
    let google_remotes_state = state.google_remotes_state();
    let metadata_state = state.metadata_state();
    render_template(&DirectoryTemplate {
        title: "Directory - BOREAL",
        active_page: "directory",
        alerts: build_alerts(&rclone_state, &google_client_state),
        status_items: build_status_items(
            &rclone_state,
            &google_client_state,
            &google_remotes_state,
            &metadata_state,
            configured_remote_count(&state.runtime, &rclone_state),
            authenticated_google_email(&state),
        ),
        poll_rclone: should_poll_ui(&rclone_state, &google_remotes_state, &metadata_state),
        summary,
        principals,
        organizations,
        remote_accounts,
        import_complete: query.imported,
        imported_created: query.created,
        imported_updated: query.updated,
        imported_rejected: query.rejected,
        name_filter: query.name_filter,
        email_filter: query.email_filter,
        type_filter: query.type_filter,
        status_filter: query.status_filter,
        departure_filter: query.departure_filter,
        organization_filter: query.organization_filter,
    })
}

async fn import_directory_csv(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Redirect, StatusCode> {
    const MAX_DIRECTORY_CSV_BYTES: usize = 10 * 1024 * 1024;
    let mut upload: Option<(String, Vec<u8>)> = None;
    while let Some(field) = multipart.next_field().await.map_err(|error| {
        log::error!("Unable to read directory CSV upload: {error}");
        StatusCode::BAD_REQUEST
    })? {
        if field.name() != Some("directory_csv") {
            continue;
        }
        let filename = field.file_name().unwrap_or("directory.csv").to_string();
        let data = field.bytes().await.map_err(|error| {
            log::error!("Unable to read directory CSV file: {error}");
            StatusCode::BAD_REQUEST
        })?;
        if data.len() > MAX_DIRECTORY_CSV_BYTES {
            log::warn!("Rejected directory CSV larger than 10 MiB");
            return Err(StatusCode::PAYLOAD_TOO_LARGE);
        }
        upload = Some((filename, data.to_vec()));
        break;
    }
    let (filename, data) = upload.ok_or(StatusCode::BAD_REQUEST)?;
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let summary =
        database::directory::import_csv(&database, &filename, &data).map_err(|error| {
            log::error!("Directory CSV import failed: filename={filename}, error={error}");
            StatusCode::BAD_REQUEST
        })?;
    log::info!(
        "Directory CSV imported: filename={filename}, rows={}, created={}, updated={}, rejected={}",
        summary.rows_seen,
        summary.rows_created,
        summary.rows_updated,
        summary.rows_rejected,
    );
    let url = format!(
        "/directory?imported=true&created={}&updated={}&rejected={}",
        summary.rows_created, summary.rows_updated, summary.rows_rejected,
    );
    Ok(Redirect::to(&url))
}

async fn principal_page(
    State(state): State<Arc<AppState>>,
    Path(principal_id): Path<i64>,
) -> Result<Html<String>, StatusCode> {
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let principal = database::directory::get_principal(&database, principal_id)
        .map_err(|error| {
            log::error!("Unable to load directory principal: {error}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    let associations = database::directory::list_principal_associations(&database, principal_id)
        .map_err(|error| {
            log::error!("Unable to load principal Drive associations: {error}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let rclone_state = state.rclone_state();
    let google_client_state = state.google_client_state();
    let google_remotes_state = state.google_remotes_state();
    let metadata_state = state.metadata_state();
    render_template(&PrincipalTemplate {
        title: format!("{} - Directory - BOREAL", principal.display_name),
        active_page: "directory",
        alerts: build_alerts(&rclone_state, &google_client_state),
        status_items: build_status_items(
            &rclone_state,
            &google_client_state,
            &google_remotes_state,
            &metadata_state,
            configured_remote_count(&state.runtime, &rclone_state),
            authenticated_google_email(&state),
        ),
        poll_rclone: should_poll_ui(&rclone_state, &google_remotes_state, &metadata_state),
        principal,
        associations,
    })
}

async fn create_tag(
    State(state): State<Arc<AppState>>,
    Form(form): Form<TagForm>,
) -> Result<Redirect, StatusCode> {
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    database::inventory::create_tag(&database, &form.name, &form.color).map_err(|error| {
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
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    database::inventory::update_tag(&database, &form.slug, &form.name, &form.color).map_err(
        |error| {
            eprintln!("Unable to update tag: {error}");
            StatusCode::BAD_REQUEST
        },
    )?;
    println!("Tag updated: slug={}", form.slug);
    Ok(Redirect::to("/tags?saved=true"))
}

fn tag_text_color(color: &str) -> &'static str {
    let value = u32::from_str_radix(color.trim_start_matches('#'), 16).unwrap_or(0x6c757d);
    let red = (value >> 16) & 0xff;
    let green = (value >> 8) & 0xff;
    let blue = value & 0xff;
    if red * 299 + green * 587 + blue * 114 > 150_000 {
        "#212529"
    } else {
        "#ffffff"
    }
}

async fn open_rclone_gui(State(state): State<Arc<AppState>>) -> Result<Redirect, StatusCode> {
    let url = rclone_gui_url(&state.rclone_state());

    if url.is_empty() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    Ok(Redirect::to(&url))
}

async fn quit(State(state): State<Arc<AppState>>) -> StatusCode {
    println!("Quit requested from WebUI.");

    state.request_shutdown();

    StatusCode::ACCEPTED
}

async fn ui_alerts(State(state): State<Arc<AppState>>) -> Result<Html<String>, StatusCode> {
    let rclone_state = state.rclone_state();

    let google_client_state = state.google_client_state();

    let template = AlertsTemplate {
        alerts: build_alerts(&rclone_state, &google_client_state),

        poll_rclone: should_poll_rclone(&rclone_state),
    };

    render_template(&template)
}

async fn ui_status(State(state): State<Arc<AppState>>) -> Result<Html<String>, StatusCode> {
    let rclone_state = state.rclone_state();

    let google_client_state = state.google_client_state();
    let google_remotes_state = state.google_remotes_state();
    let metadata_state = state.metadata_state();

    let template = StatusTemplate {
        status_items: build_status_items(
            &rclone_state,
            &google_client_state,
            &google_remotes_state,
            &metadata_state,
            configured_remote_count(&state.runtime, &rclone_state),
            authenticated_google_email(&state),
        ),

        poll_rclone: should_poll_ui(&rclone_state, &google_remotes_state, &metadata_state),
    };

    render_template(&template)
}

async fn ui_setup_progress(State(state): State<Arc<AppState>>) -> Result<Html<String>, StatusCode> {
    let rclone_state = state.rclone_state();

    let google_client_state = state.google_client_state();
    let google_remotes_state = state.google_remotes_state();

    let (setup_steps, setup_percent) =
        build_setup_progress(&rclone_state, &google_client_state, &google_remotes_state);

    let template = SetupProgressTemplate {
        setup_steps,
        setup_percent,
        poll_rclone: should_poll_setup(&rclone_state, &google_remotes_state),
    };

    render_template(&template)
}

async fn ui_drive_summaries(
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, StatusCode> {
    let metadata_state = state.metadata_state();
    let shared_summary = latest_shared_summary(&state);
    let template = DriveSummariesTemplate {
        metadata: build_metadata_view(&metadata_state, true, false, shared_summary.as_ref()),
    };
    render_template(&template)
}

async fn ui_metadata_progress(
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, StatusCode> {
    let remotes = state.google_remotes_state();
    let rclone_state = state.rclone_state();
    let available = matches!(remotes.ro, RemoteState::Ready);
    let metadata_state = state.metadata_state();
    let shared_summary = latest_shared_summary(&state);

    render_template(&MetadataProgressTemplate {
        metadata: build_metadata_view(
            &metadata_state,
            available,
            should_poll_setup(&rclone_state, &remotes),
            shared_summary.as_ref(),
        ),
    })
}

async fn ui_metadata_update_modal(
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, StatusCode> {
    let remotes = state.google_remotes_state();
    let rclone_state = state.rclone_state();
    let metadata_state = state.metadata_state();
    let shared_summary = latest_shared_summary(&state);
    let available = matches!(remotes.ro, RemoteState::Ready);
    let timing = if matches!(metadata_state, MetadataState::Updating(_)) {
        state.database().ok().and_then(|database| {
            database::inventory::scan_timing_estimate(&database, "shared-with-me")
                .ok()
                .flatten()
        })
    } else {
        None
    };
    let progress_percent = metadata_progress_percent(&metadata_state, timing.as_ref());

    render_template(&MetadataUpdateModalTemplate {
        metadata: build_metadata_view(
            &metadata_state,
            available,
            should_poll_setup(&rclone_state, &remotes),
            shared_summary.as_ref(),
        ),
        progress_percent,
        timing_available: timing.is_some(),
        elapsed_label: format_duration(timing.map(|value| value.elapsed_seconds).unwrap_or(0)),
        estimated_total_label: format_duration(
            timing.map(|value| value.average_seconds).unwrap_or(0),
        ),
        remaining_label: format_duration(
            timing
                .map(|value| value.average_seconds.saturating_sub(value.elapsed_seconds))
                .unwrap_or(0),
        ),
        timing_samples: timing.map(|value| value.sample_count).unwrap_or(0),
    })
}

fn metadata_progress_percent(
    state: &MetadataState,
    timing: Option<&database::inventory::ScanTimingEstimate>,
) -> u8 {
    let phase_percent = match state {
        MetadataState::Updating(progress) => match progress.phase {
            "Connecting" => 5,
            "Downloading directory spreadsheet" => 8,
            "Importing directory spreadsheet" => 12,
            "Fetching My Drive metadata" => 15,
            "Fetching Shared with me metadata" => 45,
            "Saving My Drive metadata" => 65,
            "Saving Shared with me metadata" => 85,
            _ => 10,
        },
        MetadataState::Synchronized(_) => 100,
        _ => 0,
    };
    let elapsed_percent = timing
        .map(|value| {
            ((value.elapsed_seconds.saturating_mul(100) / value.average_seconds).min(95)) as u8
        })
        .unwrap_or(0);
    phase_percent.max(elapsed_percent)
}

fn format_duration(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;
    if hours > 0 {
        format!("{hours}h {minutes:02}m {seconds:02}s")
    } else {
        format!("{minutes}m {seconds:02}s")
    }
}

fn latest_shared_summary(state: &AppState) -> Option<database::inventory::InventorySummary> {
    let database = state.database().ok()?;
    database::inventory::latest_summary_for(&database, "shared-with-me")
        .ok()
        .flatten()
}

async fn import_google_client(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Redirect, StatusCode> {
    let mut credentials: Option<Vec<u8>> = None;

    while let Some(field) = multipart.next_field().await.map_err(|error| {
        eprintln!("Unable to read Google Client ID upload: {error}");

        StatusCode::BAD_REQUEST
    })? {
        if field.name() != Some("credentials") {
            continue;
        }

        let data = field.bytes().await.map_err(|error| {
            eprintln!("Unable to read uploaded Google Client ID file: {error}");

            StatusCode::BAD_REQUEST
        })?;

        credentials = Some(data.to_vec());

        break;
    }

    let data = credentials.ok_or(StatusCode::BAD_REQUEST)?;

    match google::client::import(&state.runtime, &data) {
        Ok(config) => {
            println!("Google Client ID imported: {}", config.client_id);

            state.set_google_client_state(GoogleClientState::Ready(config));

            state.refresh_google_remotes_if_ready();

            Ok(Redirect::to("/"))
        }

        Err(error) => {
            let message = error.to_string();

            eprintln!("Google Client ID import failed: {message}");

            state.set_google_client_state(GoogleClientState::Error(message));

            Ok(Redirect::to("/"))
        }
    }
}

async fn setup_my_drive_ro(State(state): State<Arc<AppState>>) -> Result<Redirect, StatusCode> {
    start_remote_setup(state, RemoteKind::MyDriveRo)
}

fn start_remote_setup(state: Arc<AppState>, kind: RemoteKind) -> Result<Redirect, StatusCode> {
    AppState::configure_google_remote(state, kind).map_err(|error| {
        eprintln!("Unable to start {} setup: {error}", kind.label());
        StatusCode::CONFLICT
    })?;

    Ok(Redirect::to("/"))
}

async fn start_metadata_update(State(state): State<Arc<AppState>>) -> Result<Redirect, StatusCode> {
    let remotes = state.google_remotes_state();

    if !matches!(remotes.ro, RemoteState::Ready) {
        return Err(StatusCode::PRECONDITION_FAILED);
    }

    AppState::start_metadata_update(state).map_err(|error| {
        eprintln!("Unable to start metadata update: {error}");
        StatusCode::CONFLICT
    })?;

    Ok(Redirect::to("/"))
}

fn should_poll_rclone(rclone_state: &RcloneState) -> bool {
    matches!(rclone_state, RcloneState::Initializing)
}

fn should_poll_setup(rclone_state: &RcloneState, remotes_state: &GoogleRemotesState) -> bool {
    should_poll_rclone(rclone_state)
        || matches!(remotes_state.ro, RemoteState::Configuring)
}

fn should_poll_ui(
    rclone_state: &RcloneState,
    remotes_state: &GoogleRemotesState,
    metadata_state: &MetadataState,
) -> bool {
    should_poll_setup(rclone_state, remotes_state)
        || matches!(metadata_state, MetadataState::Updating(_))
}

fn rclone_gui_url(rclone_state: &RcloneState) -> String {
    match rclone_state {
        RcloneState::Ready(status) => status.gui_url.clone().unwrap_or_default(),

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
            alerts.push(AlertItem {
                level: "warning",
                icon: "bi-hourglass-split",
                message: "BOREAL is initializing Rclone...".to_string(),
                modal_target: "",
            });
        }

        RcloneState::Ready(_) => {}

        RcloneState::Error(error) => {
            alerts.push(AlertItem {
                level: "danger",
                icon: "bi-exclamation-triangle",
                message: format!("Rclone initialization failed: {error}"),
                modal_target: "",
            });
        }
    }

    match google_client_state {
        GoogleClientState::NotConfigured => {
            alerts.push(AlertItem {
                level: "warning",
                icon: "bi-key",
                message: "Google Client ID is not configured".to_string(),
                modal_target: "googleClientSetupModal",
            });
        }

        GoogleClientState::Ready(_) => {}

        GoogleClientState::Error(error) => {
            alerts.push(AlertItem {
                level: "danger",
                icon: "bi-key",
                message: format!("Google Client ID configuration is invalid: {error}"),
                modal_target: "googleClientSetupModal",
            });
        }
    }

    alerts
}

fn authenticated_google_email(state: &AppState) -> String {
    state
        .database()
        .ok()
        .and_then(|database| {
            database::directory::remote_account_email(&database, RemoteKind::MyDriveRo.name())
                .ok()
                .flatten()
        })
        .unwrap_or_default()
}

fn build_status_items(
    rclone_state: &RcloneState,
    google_client_state: &GoogleClientState,
    _google_remotes_state: &GoogleRemotesState,
    metadata_state: &MetadataState,
    configured_remote_count: usize,
    google_account_email: String,
) -> Vec<StatusItem> {
    let (rclone_value, rclone_value_class) = match rclone_state {
        RcloneState::Initializing => ("Initializing...".to_string(), "text-warning"),

        RcloneState::Ready(status) => (
            status
                .version
                .strip_prefix("rclone ")
                .unwrap_or(&status.version)
                .to_string(),
            "text-success",
        ),

        RcloneState::Error(_) => ("Unavailable".to_string(), "text-danger"),
    };

    let (client_id_value, client_id_value_class) = match google_client_state {
        GoogleClientState::NotConfigured => ("Not configured".to_string(), "text-warning"),

        GoogleClientState::Ready(_) => ("Configured".to_string(), "text-success"),

        GoogleClientState::Error(_) => ("Invalid".to_string(), "text-danger"),
    };

    let (remote_value, remote_class) = if configured_remote_count == 0 {
        ("0 configured".to_string(), "text-warning")
    } else {
        (
            format!("{configured_remote_count} configured"),
            "text-success",
        )
    };

    let (google_account_value, google_account_class) = if google_account_email.is_empty() {
        ("Not verified".to_string(), "text-warning")
    } else {
        (google_account_email, "text-success")
    };

    let (metadata_value, metadata_class, metadata_spinner) = match metadata_state {
        MetadataState::NotSynchronized => ("Not synchronized".to_string(), "text-warning", false),
        MetadataState::Updating(progress) => (progress.phase.to_string(), "text-primary", true),
        MetadataState::Synchronized(_) => (
            "00:00:00".to_string(),
            "boreal-metadata-age text-success",
            false,
        ),
        MetadataState::Error(_) => ("Update failed".to_string(), "text-danger", false),
    };

    vec![
        StatusItem {
            icon: "bi-folder-symlink",
            label: "Rclone",
            value: rclone_value,
            value_class: rclone_value_class,
            value_url: rclone_gui_url(rclone_state),
            spinner: false,
            age_timestamp: String::new(),
        },
        StatusItem {
            icon: "bi-google",
            label: "GDrive",
            value: google_account_value,
            value_class: google_account_class,
            value_url: String::new(),
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
            value: format!("v{}", env!("CARGO_PKG_VERSION"),),
            value_class: "text-success",
            value_url: String::new(),
            spinner: false,
            age_timestamp: String::new(),
        },
    ]
}

fn render_template<T>(template: &T) -> Result<Html<String>, StatusCode>
where
    T: Template,
{
    template.render().map(Html).map_err(|error| {
        eprintln!("Unable to render HTML template: {error}");

        StatusCode::INTERNAL_SERVER_ERROR
    })
}
