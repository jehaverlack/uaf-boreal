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
    app::{
        AppState, DownloadState, GoogleClientState, GoogleRemotesState, MetadataState, RcloneState,
    },
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
    pub progress_files_scanned: u64,
    pub progress_folders_scanned: u64,
    pub progress_permissions_scanned: u64,
    pub progress_size_label: String,
    pub errors: u64,
    pub completed_at: String,
    pub shared_drives_indexed: bool,
    pub shared_drives_count: usize,
    pub shared_drives_files_scanned: u64,
    pub shared_drives_folders_scanned: u64,
    pub shared_drives_permissions_scanned: u64,
    pub shared_drives_size_label: String,
    pub shared_drives_completed_at: String,
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
    directory_sheet_enabled: bool,
    directory_sheet_url: String,
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
    pub permissions: Vec<IdentityDisplay>,
    pub size: String,
    pub modified_at: String,
    pub owner: IdentityDisplay,
    pub drive_url: String,
    pub is_deleted: bool,
    pub size_bytes: u64,
    pub permission_count: usize,
}

#[allow(dead_code)]
pub struct IdentityDisplay {
    pub label: String,
    pub tagged: bool,
    pub unknown: bool,
    pub color: String,
    pub text_color: &'static str,
    pub tag_details: String,
    pub directory_url: String,
}

#[allow(dead_code)]
pub struct ExplorerSummary {
    pub items: usize,
    pub files: usize,
    pub folders: usize,
    pub size_bytes: u64,
    pub size_label: String,
    pub permissions: usize,
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
    title: String,
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
    directory_tags: Vec<database::inventory::Tag>,
    tag_filter: String,
    tagged_count: usize,
    untagged_count: usize,
    type_filter: String,
    size_filter: String,
    modified_filter: String,
    owner_filter: String,
    permission_filter: String,
    owner_identity_tag_filter: String,
    permission_identity_tag_filter: String,
    include_deleted: bool,
    heading: String,
    description: String,
    root_label: String,
    explorer_path: String,
    drive_id: String,
    inventory_scope: String,
    tag_action: &'static str,
    tag_remove_action: &'static str,
    summary: ExplorerSummary,
}

#[derive(Template)]
#[template(path = "partials/download-status.html", config = "askama.toml")]
struct DownloadStatusTemplate {
    status: &'static str,
    message: String,
    poll: bool,
}

pub struct SharedDriveView {
    pub drive_id: String,
    pub name: String,
    pub is_accessible: bool,
    pub last_error: String,
    pub files: u64,
    pub folders: u64,
    pub permissions_count: u64,
    pub managers: Vec<SharedDriveIdentityView>,
    pub permission_identities: Vec<SharedDriveIdentityView>,
    pub size_label: String,
    pub tags: Vec<TagPill>,
}

pub struct SharedDriveIdentityView {
    pub label: String,
    pub roles_label: String,
}

#[derive(Template)]
#[template(path = "shared-drives.html", config = "askama.toml")]
struct SharedDrivesTemplate {
    title: &'static str,
    active_page: &'static str,
    alerts: Vec<AlertItem>,
    status_items: Vec<StatusItem>,
    poll_rclone: bool,
    drives: Vec<SharedDriveView>,
    show_inaccessible: bool,
    inaccessible_count: usize,
    tags: Vec<database::inventory::Tag>,
    search: String,
    tag_filter: String,
    tagged_count: usize,
    untagged_count: usize,
    files_filter: String,
    folders_filter: String,
    size_filter: String,
    manager_filter: String,
    permissions_filter: String,
    sort: String,
    direction: String,
    error: String,
    summary: ExplorerSummary,
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
    tags: Vec<database::inventory::Tag>,
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
    tags: Vec<database::inventory::Tag>,
}

#[allow(dead_code)]
#[derive(Template)]
#[template(path = "directory-edit.html", config = "askama.toml")]
struct DirectoryEditTemplate {
    title: String,
    active_page: &'static str,
    alerts: Vec<AlertItem>,
    status_items: Vec<StatusItem>,
    poll_rclone: bool,
    heading: &'static str,
    action: String,
    principal_id: i64,
    email: String,
    display_name: String,
    principal_type: String,
    status: String,
    departure_date: String,
    organization: String,
    notes: String,
    error: String,
    principal_types: Vec<String>,
    principal_tags: Vec<database::directory::IdentityTag>,
    tags: Vec<database::inventory::Tag>,
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
    directory_sheet_enabled: bool,
    directory_sheet_url: String,
}

#[allow(dead_code)]
#[derive(Template)]
#[template(path = "partials/metadata-progress.html", config = "askama.toml")]
struct MetadataProgressTemplate {
    metadata: MetadataView,
}

#[allow(dead_code)]
#[derive(Template)]
#[template(
    path = "partials/metadata-update-modal-content.html",
    config = "askama.toml"
)]
struct MetadataUpdateModalTemplate {
    metadata: MetadataView,
    scopes: Vec<MetadataScopeProgressView>,
    directory_available: bool,
}

#[allow(dead_code)]
struct MetadataScopeProgressView {
    name: &'static str,
    selected: bool,
    active: bool,
    complete: bool,
    status: String,
    percent: u8,
    elapsed_label: String,
    estimate_label: String,
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
    drive: String,
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
    untagged: usize,
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
    owner_identity_tag: String,
    #[serde(default)]
    permission_identity_tag: String,
    #[serde(default)]
    include_deleted: bool,
    #[serde(default)]
    show_inaccessible: bool,
    #[serde(default)]
    files_filter: String,
    #[serde(default)]
    folders_filter: String,
    #[serde(default)]
    shared_drive_manager_filter: String,
    #[serde(default)]
    shared_drive_permission_filter: String,
}

#[derive(serde::Deserialize)]
struct ApplyTagForm {
    #[serde(default)]
    selected_item_ids: String,
    #[serde(default)]
    drive: String,
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
    owner_identity_tag_filter: String,
    permission_identity_tag_filter: String,
    #[serde(default)]
    include_deleted: bool,
}

#[derive(serde::Deserialize)]
struct DownloadForm {
    item_id: String,
    inventory_scope: String,
    #[serde(default)]
    drive: String,
}

#[derive(serde::Deserialize)]
struct SharedDriveTagForm {
    #[serde(default)]
    selected_drive_ids: String,
    tag: String,
    #[serde(default)]
    q: String,
    #[serde(default)]
    tag_filter: String,
    #[serde(default)]
    show_inaccessible: bool,
    #[serde(default)]
    files_filter: String,
    #[serde(default)]
    folders_filter: String,
    #[serde(default)]
    size_filter: String,
    #[serde(default)]
    manager_filter: String,
    #[serde(default)]
    permissions_filter: String,
    #[serde(default)]
    sort: String,
    #[serde(default)]
    direction: String,
}

#[derive(serde::Deserialize)]
struct ApplyPrincipalTagForm {
    #[serde(default)]
    selected_principal_ids: String,
    tag: String,
}

#[derive(serde::Deserialize)]
struct PrincipalEditForm {
    email: String,
    display_name: String,
    principal_type: String,
    status: String,
    #[serde(default)]
    departure_date: String,
    #[serde(default)]
    organization: String,
    #[serde(default)]
    notes: String,
}

#[derive(serde::Deserialize, Default)]
struct NewPrincipalQuery {
    #[serde(default)]
    email: String,
}

#[derive(serde::Deserialize)]
struct TagForm {
    #[serde(default)]
    slug: String,
    name: String,
    color: String,
    #[serde(default)]
    directory: Option<String>,
    #[serde(default)]
    my_drive: Option<String>,
    #[serde(default)]
    shared_drives: Option<String>,
    #[serde(default)]
    shared_with_me: Option<String>,
}

#[derive(serde::Deserialize)]
struct SettingsForm {
    refresh_interval_hours: u32,
    full_reconciliation_days: u32,
    #[serde(default)]
    permission_scanning: Option<String>,
    #[serde(default)]
    directory_sheet_enabled: Option<String>,
    #[serde(default)]
    directory_sheet_url: String,
}

#[derive(serde::Deserialize)]
struct SetupDirectoryForm {
    #[serde(default)]
    directory_sheet_url: String,
    #[serde(default)]
    skip: Option<String>,
}

#[derive(serde::Deserialize)]
struct MetadataUpdateForm {
    #[serde(default)]
    my_drive: Option<String>,
    #[serde(default)]
    shared_drives: Option<String>,
    #[serde(default)]
    shared_with_me: Option<String>,
    #[serde(default)]
    directory_info: Option<String>,
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
        .route("/my-drive/tags/remove", post(remove_my_drive_tag))
        .route("/shared-drives", get(shared_drives_page))
        .route(
            "/shared-drives/manage-tags",
            post(apply_shared_drive_list_tag),
        )
        .route(
            "/shared-drives/manage-tags/remove",
            post(remove_shared_drive_list_tag),
        )
        .route("/shared-drives/tags", post(apply_shared_drive_tag))
        .route("/shared-drives/tags/remove", post(remove_shared_drive_tag))
        .route("/shared-with-me", get(shared_with_me_page))
        .route("/shared-with-me/tags", post(apply_shared_with_me_tag))
        .route(
            "/shared-with-me/tags/remove",
            post(remove_shared_with_me_tag),
        )
        .route("/downloads/start", post(start_download))
        .route("/ui/download-status", get(ui_download_status))
        .route("/tags", get(tags_page))
        .route("/directory", get(directory_page))
        .route(
            "/directory/new",
            get(new_principal_page).post(create_manual_principal),
        )
        .route("/directory/principals/{principal_id}", get(principal_page))
        .route(
            "/directory/principals/{principal_id}/edit",
            get(edit_principal_page).post(update_manual_principal),
        )
        .route(
            "/directory/principals/{principal_id}/edit/tags",
            post(apply_principal_editor_tag),
        )
        .route(
            "/directory/principals/{principal_id}/edit/tags/remove",
            post(remove_principal_editor_tag),
        )
        .route("/directory/tags", post(apply_directory_tag))
        .route("/directory/tags/remove", post(remove_directory_tag))
        .route(
            "/directory/principals/{principal_id}/tags",
            post(apply_principal_tag),
        )
        .route(
            "/directory/principals/{principal_id}/tags/remove",
            post(remove_principal_tag),
        )
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
        .route("/setup/directory", post(save_setup_directory))
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

    let setup_settings = state
        .database()
        .ok()
        .and_then(|database| database::settings::load(&database).ok())
        .unwrap_or_default();
    let directory_setup_decided = setup_settings.directory_sheet_enabled
        || state
            .database()
            .ok()
            .and_then(|database| database::settings::directory_setup_skipped(&database).ok())
            .unwrap_or(false);
    let (setup_steps, setup_percent) = build_setup_progress(
        &rclone_state,
        &google_client_state,
        &google_remotes_state,
        directory_setup_decided,
    );

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
            latest_my_drive_summary(&state).as_ref(),
            shared_summary.as_ref(),
            latest_shared_drives_summary(&state).as_ref(),
            shared_drive_count(&state),
        ),
        directory_sheet_enabled: setup_settings.directory_sheet_enabled,
        directory_sheet_url: setup_settings.directory_sheet_url,
    };

    render_template(&template)
}

fn build_setup_progress(
    rclone_state: &RcloneState,
    google_client_state: &GoogleClientState,
    google_remotes_state: &GoogleRemotesState,
    directory_setup_decided: bool,
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

    let complete_count =
        steps.iter().filter(|step| step.complete).count() + usize::from(directory_setup_decided);
    let setup_percent = (complete_count * 100 / 4) as u8;

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
    my_drive_summary: Option<&database::inventory::InventorySummary>,
    shared_summary: Option<&database::inventory::InventorySummary>,
    shared_drives_summary: Option<&database::inventory::InventorySummary>,
    shared_drives_count: usize,
) -> MetadataView {
    let my_drive = my_drive_summary.cloned().unwrap_or_default();
    let shared_indexed = shared_summary.is_some();
    let shared = shared_summary.cloned().unwrap_or_default();
    let shared_drives_indexed = shared_drives_summary.is_some();
    let shared_drives = shared_drives_summary.cloned().unwrap_or_default();
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
            progress_files_scanned: 0,
            progress_folders_scanned: 0,
            progress_permissions_scanned: 0,
            progress_size_label: "0 B".to_string(),
            errors: 0,
            completed_at: String::new(),
            shared_drives_indexed,
            shared_drives_count,
            shared_drives_files_scanned: shared_drives.files_scanned,
            shared_drives_folders_scanned: shared_drives.folders_scanned,
            shared_drives_permissions_scanned: shared_drives.permissions_scanned,
            shared_drives_size_label: format_bytes(shared_drives.bytes_discovered),
            shared_drives_completed_at: shared_drives.completed_at.clone(),
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
            files_scanned: my_drive.files_scanned,
            folders_scanned: my_drive.folders_scanned,
            permissions_scanned: my_drive.permissions_scanned,
            size_label: format_bytes(my_drive.bytes_discovered),
            progress_files_scanned: progress.files_scanned,
            progress_folders_scanned: progress.folders_scanned,
            progress_permissions_scanned: progress.permissions_scanned,
            progress_size_label: format_bytes(progress.bytes_discovered),
            errors: progress.errors,
            completed_at: my_drive.completed_at.clone(),
            shared_drives_indexed,
            shared_drives_count,
            shared_drives_files_scanned: shared_drives.files_scanned,
            shared_drives_folders_scanned: shared_drives.folders_scanned,
            shared_drives_permissions_scanned: shared_drives.permissions_scanned,
            shared_drives_size_label: format_bytes(shared_drives.bytes_discovered),
            shared_drives_completed_at: shared_drives.completed_at.clone(),
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
            progress_files_scanned: summary.files_scanned,
            progress_folders_scanned: summary.folders_scanned,
            progress_permissions_scanned: summary.permissions_scanned,
            progress_size_label: format_bytes(summary.bytes_discovered),
            errors: 0,
            completed_at: summary.completed_at.clone(),
            shared_drives_indexed,
            shared_drives_count,
            shared_drives_files_scanned: shared_drives.files_scanned,
            shared_drives_folders_scanned: shared_drives.folders_scanned,
            shared_drives_permissions_scanned: shared_drives.permissions_scanned,
            shared_drives_size_label: format_bytes(shared_drives.bytes_discovered),
            shared_drives_completed_at: shared_drives.completed_at.clone(),
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
            files_scanned: my_drive.files_scanned,
            folders_scanned: my_drive.folders_scanned,
            permissions_scanned: my_drive.permissions_scanned,
            size_label: format_bytes(my_drive.bytes_discovered),
            progress_files_scanned: 0,
            progress_folders_scanned: 0,
            progress_permissions_scanned: 0,
            progress_size_label: "0 B".to_string(),
            errors: 1,
            completed_at: my_drive.completed_at.clone(),
            shared_drives_indexed,
            shared_drives_count,
            shared_drives_files_scanned: shared_drives.files_scanned,
            shared_drives_folders_scanned: shared_drives.folders_scanned,
            shared_drives_permissions_scanned: shared_drives.permissions_scanned,
            shared_drives_size_label: format_bytes(shared_drives.bytes_discovered),
            shared_drives_completed_at: shared_drives.completed_at.clone(),
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

    render_settings(
        &state,
        inventory_settings,
        query.saved,
        String::new(),
        String::new(),
    )
}

async fn save_settings(
    State(state): State<Arc<AppState>>,
    Form(form): Form<SettingsForm>,
) -> Result<axum::response::Response, StatusCode> {
    let inventory_settings = InventorySettings {
        automatic_updates: false,
        refresh_interval_hours: form.refresh_interval_hours,
        full_reconciliation_days: form.full_reconciliation_days,
        update_when_overdue_at_startup: false,
        permission_scanning: form.permission_scanning.is_some(),
        directory_sheet_enabled: form.directory_sheet_enabled.is_some(),
        directory_sheet_url: form.directory_sheet_url.trim().to_string(),
    };
    if inventory_settings.directory_sheet_enabled {
        if let Err(error) =
            crate::rclone::identity::parse_google_sheet_url(&inventory_settings.directory_sheet_url)
        {
            return render_settings(
                &state,
                inventory_settings,
                false,
                error.to_string(),
                String::new(),
            )
            .map(axum::response::IntoResponse::into_response);
        }
    }
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    match settings::save(&database, &inventory_settings) {
        Ok(()) => Ok(Redirect::to("/settings?saved=true").into_response()),

        Err(error) => render_settings(
            &state,
            inventory_settings,
            false,
            error.to_string(),
            String::new(),
        )
        .map(axum::response::IntoResponse::into_response),
    }
}

async fn test_directory_sheet(
    State(state): State<Arc<AppState>>,
    Form(form): Form<SettingsForm>,
) -> Result<axum::response::Response, StatusCode> {
    let inventory_settings = InventorySettings {
        automatic_updates: false,
        refresh_interval_hours: form.refresh_interval_hours,
        full_reconciliation_days: form.full_reconciliation_days,
        update_when_overdue_at_startup: false,
        permission_scanning: form.permission_scanning.is_some(),
        directory_sheet_enabled: form.directory_sheet_enabled.is_some(),
        directory_sheet_url: form.directory_sheet_url.trim().to_string(),
    };
    if let Err(error) =
        crate::rclone::identity::parse_google_sheet_url(&inventory_settings.directory_sheet_url)
    {
        return render_settings(
            &state,
            inventory_settings,
            false,
            error.to_string(),
            String::new(),
        )
        .map(axum::response::IntoResponse::into_response);
    }
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if let Err(error) = settings::save(&database, &inventory_settings) {
        return render_settings(
            &state,
            inventory_settings,
            false,
            error.to_string(),
            String::new(),
        )
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
            .map(axum::response::IntoResponse::into_response);
        }
    };
    let result: Result<(), String> = tokio::task::spawn_blocking(move || {
        crate::rclone::identity::fetch_read_only_account(&worker_state.runtime, &rclone_path)
            .map_err(|error| error.to_string())?;
        let (_, csv) =
            crate::rclone::identity::download_google_sheet_csv(&worker_state.runtime, &url)
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
        "/my-drive/tags/remove",
        String::new(),
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
        "/shared-with-me/tags/remove",
        String::new(),
    )
}

async fn shared_drives_page(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DrivePathQuery>,
) -> Result<Html<String>, StatusCode> {
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if query.drive.is_empty() {
        let (all_drives, error) = match database::inventory::list_shared_drives_filtered(
            &database,
            &query.q,
            &query.tag,
            &query.files_filter,
            &query.folders_filter,
            &query.size_filter,
            &query.shared_drive_manager_filter,
            &query.shared_drive_permission_filter,
        ) {
            Ok(drives) => (drives, String::new()),
            Err(error) => (Vec::new(), error.to_string()),
        };
        let inaccessible_count = all_drives
            .iter()
            .filter(|drive| !drive.is_accessible)
            .count();
        let mut filtered_drives = all_drives
            .into_iter()
            .filter(|drive| query.show_inaccessible || drive.is_accessible)
            .collect::<Vec<_>>();
        let sort = match query.sort.as_str() {
            "tags" | "files" | "folders" | "size" | "managers" | "permissions" => {
                query.sort.clone()
            }
            _ => "name".to_string(),
        };
        filtered_drives.sort_by(|left, right| {
            let ordering = match sort.as_str() {
                "tags" => left
                    .tags
                    .first()
                    .map(|tag| tag.name.to_ascii_lowercase())
                    .cmp(&right.tags.first().map(|tag| tag.name.to_ascii_lowercase())),
                "files" => left.files_scanned.cmp(&right.files_scanned),
                "folders" => left.folders_scanned.cmp(&right.folders_scanned),
                "size" => left.bytes_discovered.cmp(&right.bytes_discovered),
                "managers" => {
                    shared_drive_manager_sort_key(left).cmp(&shared_drive_manager_sort_key(right))
                }
                "permissions" => left
                    .permission_identities
                    .first()
                    .map(|identity| identity.label.to_ascii_lowercase())
                    .cmp(
                        &right
                            .permission_identities
                            .first()
                            .map(|identity| identity.label.to_ascii_lowercase()),
                    ),
                _ => left
                    .name
                    .to_ascii_lowercase()
                    .cmp(&right.name.to_ascii_lowercase()),
            };
            if query.direction.eq_ignore_ascii_case("desc") {
                ordering.reverse()
            } else {
                ordering
            }
            .then_with(|| left.drive_id.cmp(&right.drive_id))
        });
        let summary_size = filtered_drives
            .iter()
            .map(|drive| drive.bytes_discovered)
            .sum::<u64>();
        let summary = ExplorerSummary {
            items: filtered_drives.len(),
            files: filtered_drives
                .iter()
                .map(|drive| drive.files_scanned as usize)
                .sum(),
            folders: filtered_drives
                .iter()
                .map(|drive| drive.folders_scanned as usize)
                .sum(),
            size_bytes: summary_size,
            size_label: format_bytes(summary_size),
            permissions: filtered_drives
                .iter()
                .map(|drive| drive.permissions_scanned as usize)
                .sum(),
        };
        let drives = filtered_drives
            .into_iter()
            .map(|drive| {
                let managers = drive
                    .permission_identities
                    .iter()
                    .filter(|identity| {
                        identity.roles.iter().any(|role| {
                            role.eq_ignore_ascii_case("organizer")
                                || role.eq_ignore_ascii_case("owner")
                        })
                    })
                    .map(shared_drive_identity_view)
                    .collect();
                let permission_identities = drive
                    .permission_identities
                    .iter()
                    .map(shared_drive_identity_view)
                    .collect();
                SharedDriveView {
                    drive_id: drive.drive_id,
                    name: drive.name,
                    is_accessible: drive.is_accessible,
                    last_error: drive.last_error,
                    files: drive.files_scanned,
                    folders: drive.folders_scanned,
                    permissions_count: drive.permissions_scanned,
                    managers,
                    permission_identities,
                    size_label: format_bytes(drive.bytes_discovered),
                    tags: drive
                        .tags
                        .into_iter()
                        .map(|tag| TagPill {
                            text_color: tag_text_color(&tag.color),
                            name: tag.name,
                            color: tag.color,
                        })
                        .collect(),
                }
            })
            .collect();
        let tags = database::inventory::list_tags_for_scope(
            &database,
            database::inventory::TagScope::SharedDrives,
        )
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let rclone_state = state.rclone_state();
        let google_client_state = state.google_client_state();
        let google_remotes_state = state.google_remotes_state();
        let metadata_state = state.metadata_state();
        return render_template(&SharedDrivesTemplate {
            title: "Shared Drives - BOREAL",
            active_page: "shared-drives",
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
            drives,
            show_inaccessible: query.show_inaccessible,
            inaccessible_count,
            tags,
            search: query.q,
            tag_filter: query.tag,
            tagged_count: query.tagged,
            untagged_count: query.untagged,
            files_filter: query.files_filter,
            folders_filter: query.folders_filter,
            size_filter: query.size_filter,
            manager_filter: query.shared_drive_manager_filter,
            permissions_filter: query.shared_drive_permission_filter,
            sort,
            direction: if query.direction.eq_ignore_ascii_case("desc") {
                "desc".to_string()
            } else {
                "asc".to_string()
            },
            error,
            summary,
        });
    }
    let drive = database::inventory::get_shared_drive(&database, &query.drive)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let explorer_path = format!(
        "/shared-drives?drive={}",
        encode_query_value(&drive.drive_id)
    );
    render_drive_explorer(
        &state,
        query,
        &drive.inventory_scope,
        "shared-drives",
        &format!("{} — Shared Drive Explorer", drive.name),
        "Browse the latest local metadata inventory for this Shared Drive.",
        &drive.name,
        &explorer_path,
        "/shared-drives/tags",
        "/shared-drives/tags/remove",
        drive.drive_id,
    )
}

fn shared_drive_manager_sort_key(drive: &database::inventory::SharedDriveRow) -> Option<String> {
    drive
        .permission_identities
        .iter()
        .filter(|identity| {
            identity.roles.iter().any(|role| {
                role.eq_ignore_ascii_case("organizer") || role.eq_ignore_ascii_case("owner")
            })
        })
        .map(|identity| identity.label.to_ascii_lowercase())
        .min()
}

fn shared_drive_identity_view(
    identity: &database::inventory::SharedDrivePermissionIdentity,
) -> SharedDriveIdentityView {
    let roles_label = identity
        .roles
        .iter()
        .map(|role| match role.to_ascii_lowercase().as_str() {
            "organizer" => "Manager".to_string(),
            "fileorganizer" => "Content manager".to_string(),
            "writer" => "Contributor".to_string(),
            "commenter" => "Commenter".to_string(),
            "reader" => "Viewer".to_string(),
            "owner" => "Owner".to_string(),
            _ => role.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ");
    SharedDriveIdentityView {
        label: identity.label.clone(),
        roles_label,
    }
}

fn render_drive_explorer(
    state: &AppState,
    query: DrivePathQuery,
    inventory_scope: &str,
    active_page: &'static str,
    heading: &str,
    description: &str,
    root_label: &str,
    explorer_path: &str,
    tag_action: &'static str,
    tag_remove_action: &'static str,
    drive_id: String,
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
        &query.owner_identity_tag,
        &query.permission_identity_tag,
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
    let summary_size = items.iter().filter_map(|item| item.size_bytes).sum::<u64>();
    let summary = ExplorerSummary {
        items: items.len(),
        files: items.iter().filter(|item| !item.is_directory).count(),
        folders: items.iter().filter(|item| item.is_directory).count(),
        size_bytes: summary_size,
        size_label: format_bytes(summary_size),
        permissions: items.iter().map(|item| item.permissions.len()).sum(),
    };
    let rows = items
        .into_iter()
        .map(|item| {
            let size_bytes = item.size_bytes.unwrap_or(0);
            let permission_count = item.permissions.len();
            let owner = identity_display(
                item.owner_email.clone().unwrap_or_else(|| "—".to_string()),
                item.owner_known,
                &item.owner_tags,
            );
            let permissions = item
                .permissions
                .iter()
                .map(|permission| {
                    identity_display(permission.label.clone(), permission.known, &permission.tags)
                })
                .collect();
            DriveExplorerRow {
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
                        &query.owner_identity_tag,
                        &query.permission_identity_tag,
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
                permissions,
                size: item
                    .size_bytes
                    .map(format_bytes)
                    .unwrap_or_else(|| "—".to_string()),
                modified_at: item.modified_at.unwrap_or_else(|| "—".to_string()),
                owner,
                is_deleted: item.is_deleted,
                size_bytes,
                permission_count,
            }
        })
        .collect();
    let parent_path = query
        .path
        .rsplit_once('/')
        .map(|(parent, _)| parent.to_string())
        .unwrap_or_default();
    let tag_scope = database::inventory::TagScope::for_inventory(inventory_scope)
        .ok_or(StatusCode::BAD_REQUEST)?;
    let tags = database::inventory::list_tags_for_scope(&database, tag_scope).map_err(|error| {
        eprintln!("Unable to load My Drive tags: {error}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let directory_tags = database::inventory::list_tags_for_scope(
        &database,
        database::inventory::TagScope::Directory,
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let template = MyDriveTemplate {
        title: heading.to_string(),
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
            "",
            "",
            sort,
            if descending { "desc" } else { "asc" },
            false,
        ),
        tags,
        directory_tags,
        tag_filter: query.tag,
        tagged_count: query.tagged,
        untagged_count: query.untagged,
        type_filter: query.type_filter,
        size_filter: query.size_filter,
        modified_filter: query.modified_filter,
        owner_filter: query.owner_filter,
        permission_filter: query.permission_filter,
        owner_identity_tag_filter: query.owner_identity_tag,
        permission_identity_tag_filter: query.permission_identity_tag,
        include_deleted: query.include_deleted,
        heading: heading.to_string(),
        description: description.to_string(),
        root_label: root_label.to_string(),
        explorer_path: explorer_path.to_string(),
        drive_id,
        inventory_scope: inventory_scope.to_string(),
        tag_action,
        tag_remove_action,
        summary,
    };
    render_template(&template)
}

fn identity_display(
    label: String,
    known: bool,
    tags: &[database::inventory::Tag],
) -> IdentityDisplay {
    let first = tags.first();
    let unknown = !known && label != "—" && !label.trim().is_empty();
    IdentityDisplay {
        directory_url: if unknown && label.contains('@') {
            format!("/directory/new?email={}", encode_query_value(&label))
        } else {
            String::new()
        },
        label,
        tagged: first.is_some(),
        unknown,
        color: first.map(|tag| tag.color.clone()).unwrap_or_default(),
        text_color: first
            .map(|tag| tag_text_color(&tag.color))
            .unwrap_or("#212529"),
        tag_details: if unknown {
            "Unknown identity — not found in BOREAL Persons".to_string()
        } else if tags.is_empty() {
            "No identity tags".to_string()
        } else {
            format!(
                "Identity tags: {}",
                tags.iter()
                    .map(|tag| tag.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        },
    }
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
    owner_identity_tag_filter: &str,
    permission_identity_tag_filter: &str,
    sort: &str,
    direction: &str,
    include_deleted: bool,
) -> String {
    format!(
        "{explorer_path}{}path={}&q={}&tag={}&type_filter={}&size_filter={}&modified_filter={}&owner_filter={}&permission_filter={}&owner_identity_tag={}&permission_identity_tag={}&sort={}&direction={}&include_deleted={include_deleted}",
        if explorer_path.contains('?') {
            "&"
        } else {
            "?"
        },
        encode_query_value(path),
        encode_query_value(search),
        encode_query_value(tag),
        encode_query_value(type_filter),
        encode_query_value(size_filter),
        encode_query_value(modified_filter),
        encode_query_value(owner_filter),
        encode_query_value(permission_filter),
        encode_query_value(owner_identity_tag_filter),
        encode_query_value(permission_identity_tag_filter),
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
        &query.owner_identity_tag,
        &query.permission_identity_tag,
        requested_sort,
        next_direction,
        query.include_deleted,
    )
}

async fn apply_my_drive_tag(
    State(state): State<Arc<AppState>>,
    Form(form): Form<ApplyTagForm>,
) -> Result<Redirect, StatusCode> {
    change_drive_tag(
        &state,
        form,
        database::inventory::MY_DRIVE_SCOPE,
        "/my-drive",
        false,
    )
}

async fn apply_shared_with_me_tag(
    State(state): State<Arc<AppState>>,
    Form(form): Form<ApplyTagForm>,
) -> Result<Redirect, StatusCode> {
    change_drive_tag(
        &state,
        form,
        database::inventory::SHARED_WITH_ME_SCOPE,
        "/shared-with-me",
        false,
    )
}

async fn apply_shared_drive_tag(
    State(state): State<Arc<AppState>>,
    Form(form): Form<ApplyTagForm>,
) -> Result<Redirect, StatusCode> {
    change_shared_drive_tag(&state, form, false)
}

async fn apply_shared_drive_list_tag(
    State(state): State<Arc<AppState>>,
    Form(form): Form<SharedDriveTagForm>,
) -> Result<Redirect, StatusCode> {
    change_shared_drive_list_tag(&state, form, false)
}

async fn remove_shared_drive_list_tag(
    State(state): State<Arc<AppState>>,
    Form(form): Form<SharedDriveTagForm>,
) -> Result<Redirect, StatusCode> {
    change_shared_drive_list_tag(&state, form, true)
}

fn change_shared_drive_list_tag(
    state: &AppState,
    form: SharedDriveTagForm,
    remove: bool,
) -> Result<Redirect, StatusCode> {
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let drive_ids = form
        .selected_drive_ids
        .split(',')
        .map(str::trim)
        .filter(|drive_id| !drive_id.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    let changed =
        database::inventory::change_shared_drive_tags(&database, &drive_ids, &form.tag, remove)
            .map_err(|error| {
                eprintln!("Unable to change Shared Drive tag: {error}");
                StatusCode::BAD_REQUEST
            })?;
    println!(
        "Shared Drive list tag {}: tag={}, selected_drives={}, changed_drives={changed}",
        if remove { "removed" } else { "applied" },
        form.tag,
        drive_ids.len(),
    );
    let url = format!(
        "/shared-drives?q={}&tag={}&show_inaccessible={}&files_filter={}&folders_filter={}&size_filter={}&shared_drive_manager_filter={}&shared_drive_permission_filter={}&sort={}&direction={}&{}={changed}",
        encode_query_value(&form.q),
        encode_query_value(&form.tag_filter),
        form.show_inaccessible,
        encode_query_value(&form.files_filter),
        encode_query_value(&form.folders_filter),
        encode_query_value(&form.size_filter),
        encode_query_value(&form.manager_filter),
        encode_query_value(&form.permissions_filter),
        encode_query_value(&form.sort),
        encode_query_value(&form.direction),
        if remove { "untagged" } else { "tagged" },
    );
    Ok(Redirect::to(&url))
}

async fn remove_shared_drive_tag(
    State(state): State<Arc<AppState>>,
    Form(form): Form<ApplyTagForm>,
) -> Result<Redirect, StatusCode> {
    change_shared_drive_tag(&state, form, true)
}

fn change_shared_drive_tag(
    state: &AppState,
    form: ApplyTagForm,
    remove: bool,
) -> Result<Redirect, StatusCode> {
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let drive = database::inventory::get_shared_drive(&database, &form.drive)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::BAD_REQUEST)?;
    let explorer_path = format!(
        "/shared-drives?drive={}",
        encode_query_value(&drive.drive_id)
    );
    change_drive_tag(state, form, &drive.inventory_scope, &explorer_path, remove)
}

async fn remove_my_drive_tag(
    State(state): State<Arc<AppState>>,
    Form(form): Form<ApplyTagForm>,
) -> Result<Redirect, StatusCode> {
    change_drive_tag(
        &state,
        form,
        database::inventory::MY_DRIVE_SCOPE,
        "/my-drive",
        true,
    )
}

async fn remove_shared_with_me_tag(
    State(state): State<Arc<AppState>>,
    Form(form): Form<ApplyTagForm>,
) -> Result<Redirect, StatusCode> {
    change_drive_tag(
        &state,
        form,
        database::inventory::SHARED_WITH_ME_SCOPE,
        "/shared-with-me",
        true,
    )
}

fn change_drive_tag(
    state: &AppState,
    form: ApplyTagForm,
    inventory_scope: &str,
    explorer_path: &str,
    remove: bool,
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
    let changed = if remove {
        database::inventory::remove_tag_recursively_for_scope(
            &database,
            inventory_scope,
            &selected_items,
            &form.tag,
        )
    } else {
        database::inventory::apply_tag_recursively_for_scope(
            &database,
            inventory_scope,
            &selected_items,
            &form.tag,
        )
    }
    .map_err(|error| {
        eprintln!("Unable to change Drive tag: {error}");
        StatusCode::BAD_REQUEST
    })?;
    println!(
        "Drive tag {}: tag={}, selected_items={}, changed_items={changed}",
        if remove { "removed" } else { "applied" },
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
        &form.owner_identity_tag_filter,
        &form.permission_identity_tag_filter,
        &form.sort,
        &form.direction,
        form.include_deleted,
    );
    url.push_str(&format!(
        "&{}={changed}",
        if remove { "untagged" } else { "tagged" }
    ));
    Ok(Redirect::to(&url))
}

async fn start_download(
    State(state): State<Arc<AppState>>,
    Form(form): Form<DownloadForm>,
) -> Result<Html<String>, StatusCode> {
    if matches!(state.download_state(), DownloadState::Running { .. }) {
        return render_download_status(&state.download_state());
    }
    let executable = match state.rclone_state() {
        RcloneState::Ready(status) => status.path,
        _ => {
            return render_download_message(
                "error",
                "Rclone must be ready before a download can start.".to_string(),
                false,
            );
        }
    };
    let shared_drive_id = if let Some(id) = form
        .inventory_scope
        .strip_prefix(database::inventory::SHARED_DRIVE_SCOPE_PREFIX)
    {
        if id.is_empty() || id != form.drive {
            return Err(StatusCode::BAD_REQUEST);
        }
        Some(id.to_string())
    } else if form.inventory_scope == database::inventory::MY_DRIVE_SCOPE
        || form.inventory_scope == database::inventory::SHARED_WITH_ME_SCOPE
    {
        None
    } else {
        return Err(StatusCode::BAD_REQUEST);
    };
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let item = database::inventory::get_drive_download_item(
        &database,
        &form.inventory_scope,
        &form.item_id,
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    if item.is_deleted {
        return render_download_message(
            "error",
            "Deleted inventory items cannot be downloaded.".to_string(),
            false,
        );
    }

    let selected_folder = rfd::FileDialog::new()
        .set_title("Choose BOREAL download destination")
        .pick_folder();
    let Some(selected_folder) = selected_folder else {
        return render_download_message("cancelled", String::new(), false);
    };
    let destination = selected_folder.join(safe_download_name(&item.name));
    let destination_label = destination.display().to_string();
    let config_path =
        rclone::config::path(&state.runtime).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    state.set_download_state(DownloadState::Running {
        item_name: item.name.clone(),
        destination: destination_label.clone(),
    });
    let worker_state = Arc::clone(&state);
    let item_name = item.name;
    let shared_with_me = form.inventory_scope == database::inventory::SHARED_WITH_ME_SCOPE;
    tokio::spawn(async move {
        let result = tokio::task::spawn_blocking(move || {
            rclone::download::copy_item(rclone::download::DownloadRequest {
                executable: &executable,
                config_path: &config_path,
                relative_path: &item.relative_path,
                destination: &destination,
                is_directory: item.is_directory,
                shared_with_me,
                shared_drive_id: shared_drive_id.as_deref(),
            })
        })
        .await;
        match result {
            Ok(Ok(())) => worker_state.set_download_state(DownloadState::Complete {
                item_name,
                destination: destination_label,
            }),
            Ok(Err(error)) => worker_state.set_download_state(DownloadState::Error {
                item_name,
                message: error.to_string(),
            }),
            Err(error) => worker_state.set_download_state(DownloadState::Error {
                item_name,
                message: format!("Download task failed: {error}"),
            }),
        }
    });
    render_download_status(&state.download_state())
}

async fn ui_download_status(
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, StatusCode> {
    render_download_status(&state.download_state())
}

fn render_download_status(state: &DownloadState) -> Result<Html<String>, StatusCode> {
    match state {
        DownloadState::Idle => render_download_message("idle", String::new(), false),
        DownloadState::Running {
            item_name,
            destination,
        } => render_download_message(
            "running",
            format!("Downloading {item_name} to {destination}…"),
            true,
        ),
        DownloadState::Complete {
            item_name,
            destination,
        } => render_download_message(
            "complete",
            format!("Downloaded {item_name} to {destination}."),
            false,
        ),
        DownloadState::Error { item_name, message } => render_download_message(
            "error",
            if item_name.is_empty() {
                message.clone()
            } else {
                format!("Unable to download {item_name}: {message}")
            },
            false,
        ),
    }
}

fn render_download_message(
    status: &'static str,
    message: String,
    poll: bool,
) -> Result<Html<String>, StatusCode> {
    render_template(&DownloadStatusTemplate {
        status,
        message,
        poll,
    })
}

fn safe_download_name(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => character,
        })
        .collect::<String>();
    let sanitized = sanitized.trim().trim_matches('.');
    if sanitized.is_empty() {
        "Drive item".to_string()
    } else if matches!(
        sanitized
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase()
            .as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    ) {
        format!("_{sanitized}")
    } else {
        sanitized.to_string()
    }
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
    let tags = database::inventory::list_tags_for_scope(
        &database,
        database::inventory::TagScope::Directory,
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let rclone_state = state.rclone_state();
    let google_client_state = state.google_client_state();
    let google_remotes_state = state.google_remotes_state();
    let metadata_state = state.metadata_state();
    render_template(&DirectoryTemplate {
        title: "Persons - BOREAL",
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
        tags,
    })
}

async fn apply_directory_tag(
    State(state): State<Arc<AppState>>,
    Form(form): Form<ApplyPrincipalTagForm>,
) -> Result<Redirect, StatusCode> {
    let principal_ids = form
        .selected_principal_ids
        .split(',')
        .filter_map(|value| value.trim().parse::<i64>().ok())
        .collect::<Vec<_>>();
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    database::directory::apply_principal_tag(&database, &principal_ids, &form.tag).map_err(
        |error| {
            log::error!("Unable to apply directory identity tag: {error}");
            StatusCode::BAD_REQUEST
        },
    )?;
    Ok(Redirect::to("/directory"))
}

async fn remove_directory_tag(
    State(state): State<Arc<AppState>>,
    Form(form): Form<ApplyPrincipalTagForm>,
) -> Result<Redirect, StatusCode> {
    let principal_ids = form
        .selected_principal_ids
        .split(',')
        .filter_map(|value| value.trim().parse::<i64>().ok())
        .collect::<Vec<_>>();
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    database::directory::remove_principal_tag(&database, &principal_ids, &form.tag)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Redirect::to("/directory"))
}

async fn apply_principal_tag(
    State(state): State<Arc<AppState>>,
    Path(principal_id): Path<i64>,
    Form(form): Form<ApplyPrincipalTagForm>,
) -> Result<Redirect, StatusCode> {
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    database::directory::apply_principal_tag(&database, &[principal_id], &form.tag).map_err(
        |error| {
            log::error!("Unable to apply identity tag: {error}");
            StatusCode::BAD_REQUEST
        },
    )?;
    Ok(Redirect::to(&format!(
        "/directory/principals/{principal_id}"
    )))
}

async fn remove_principal_tag(
    State(state): State<Arc<AppState>>,
    Path(principal_id): Path<i64>,
    Form(form): Form<ApplyPrincipalTagForm>,
) -> Result<Redirect, StatusCode> {
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    database::directory::remove_principal_tag(&database, &[principal_id], &form.tag)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Redirect::to(&format!(
        "/directory/principals/{principal_id}"
    )))
}

async fn apply_principal_editor_tag(
    State(state): State<Arc<AppState>>,
    Path(principal_id): Path<i64>,
    Form(form): Form<ApplyPrincipalTagForm>,
) -> Result<Redirect, StatusCode> {
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    database::directory::apply_principal_tag(&database, &[principal_id], &form.tag)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Redirect::to(&format!(
        "/directory/principals/{principal_id}/edit"
    )))
}

async fn remove_principal_editor_tag(
    State(state): State<Arc<AppState>>,
    Path(principal_id): Path<i64>,
    Form(form): Form<ApplyPrincipalTagForm>,
) -> Result<Redirect, StatusCode> {
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    database::directory::remove_principal_tag(&database, &[principal_id], &form.tag)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Redirect::to(&format!(
        "/directory/principals/{principal_id}/edit"
    )))
}

async fn new_principal_page(
    State(state): State<Arc<AppState>>,
    Query(query): Query<NewPrincipalQuery>,
) -> Result<Html<String>, StatusCode> {
    let submitted = (!query.email.trim().is_empty()).then(|| {
        (
            None,
            PrincipalEditForm {
                email: query.email.trim().to_string(),
                display_name: String::new(),
                principal_type: "person".to_string(),
                status: "active".to_string(),
                departure_date: String::new(),
                organization: String::new(),
                notes: String::new(),
            },
        )
    });
    render_principal_editor(&state, None, submitted, String::new())
}

async fn edit_principal_page(
    State(state): State<Arc<AppState>>,
    Path(principal_id): Path<i64>,
) -> Result<Html<String>, StatusCode> {
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let principal = database::directory::get_principal(&database, principal_id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    render_principal_editor(&state, Some(principal), None, String::new())
}

async fn create_manual_principal(
    State(state): State<Arc<AppState>>,
    Form(form): Form<PrincipalEditForm>,
) -> Result<axum::response::Response, StatusCode> {
    save_principal_editor(&state, None, form)
}

async fn update_manual_principal(
    State(state): State<Arc<AppState>>,
    Path(principal_id): Path<i64>,
    Form(form): Form<PrincipalEditForm>,
) -> Result<axum::response::Response, StatusCode> {
    save_principal_editor(&state, Some(principal_id), form)
}

fn save_principal_editor(
    state: &AppState,
    principal_id: Option<i64>,
    form: PrincipalEditForm,
) -> Result<axum::response::Response, StatusCode> {
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    match database::directory::save_manual_principal(
        &database,
        principal_id,
        &form.email,
        &form.display_name,
        &form.principal_type,
        &form.status,
        &form.departure_date,
        &form.organization,
        &form.notes,
    ) {
        Ok(id) => Ok(Redirect::to(&format!("/directory/principals/{id}")).into_response()),
        Err(error) => {
            render_principal_editor(state, None, Some((principal_id, form)), error.to_string())
                .map(axum::response::IntoResponse::into_response)
        }
    }
}

fn render_principal_editor(
    state: &AppState,
    principal: Option<database::directory::PrincipalRow>,
    submitted: Option<(Option<i64>, PrincipalEditForm)>,
    error: String,
) -> Result<Html<String>, StatusCode> {
    let principal_id = submitted
        .as_ref()
        .and_then(|value| value.0)
        .or_else(|| principal.as_ref().map(|value| value.id))
        .unwrap_or(0);
    let is_new = principal_id == 0;
    let (email, display_name, principal_type, status, departure_date, organization, notes) =
        if let Some((_, form)) = submitted {
            (
                form.email,
                form.display_name,
                form.principal_type,
                form.status,
                form.departure_date,
                form.organization,
                form.notes,
            )
        } else if let Some(principal) = principal {
            (
                principal.primary_email,
                principal.display_name,
                principal.principal_type,
                principal.status,
                principal.departure_date,
                principal.organizations,
                principal.notes,
            )
        } else {
            (
                String::new(),
                String::new(),
                "person".to_string(),
                "active".to_string(),
                String::new(),
                String::new(),
                String::new(),
            )
        };
    let rclone_state = state.rclone_state();
    let google_client_state = state.google_client_state();
    let google_remotes_state = state.google_remotes_state();
    let metadata_state = state.metadata_state();
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let principal_types = database::directory::list_principal_types(&database)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let tags = database::inventory::list_tags_for_scope(
        &database,
        database::inventory::TagScope::Directory,
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let principal_tags = if principal_id > 0 {
        database::directory::get_principal(&database, principal_id)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .map(|principal| principal.tags)
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    render_template(&DirectoryEditTemplate {
        title: if is_new {
            "New Person - BOREAL".to_string()
        } else {
            "Edit Person - BOREAL".to_string()
        },
        active_page: "directory",
        alerts: build_alerts(&rclone_state, &google_client_state),
        status_items: build_status_items(
            &rclone_state,
            &google_client_state,
            &google_remotes_state,
            &metadata_state,
            configured_remote_count(&state.runtime, &rclone_state),
            authenticated_google_email(state),
        ),
        poll_rclone: should_poll_ui(&rclone_state, &google_remotes_state, &metadata_state),
        heading: if is_new { "Add person" } else { "Edit person" },
        action: if is_new {
            "/directory/new".to_string()
        } else {
            format!("/directory/principals/{principal_id}/edit")
        },
        principal_id,
        email,
        display_name,
        principal_type,
        status,
        departure_date,
        organization,
        notes,
        error,
        principal_types,
        principal_tags,
        tags,
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
    let tags = database::inventory::list_tags_for_scope(
        &database,
        database::inventory::TagScope::Directory,
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let rclone_state = state.rclone_state();
    let google_client_state = state.google_client_state();
    let google_remotes_state = state.google_remotes_state();
    let metadata_state = state.metadata_state();
    render_template(&PrincipalTemplate {
        title: format!("{} - Persons - BOREAL", principal.display_name),
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
        tags,
    })
}

async fn create_tag(
    State(state): State<Arc<AppState>>,
    Form(form): Form<TagForm>,
) -> Result<Redirect, StatusCode> {
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let scopes = tag_form_scopes(&form);
    database::inventory::create_tag_with_scopes(&database, &form.name, &form.color, &scopes)
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
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let scopes = tag_form_scopes(&form);
    database::inventory::update_tag_with_scopes(
        &database,
        &form.slug,
        &form.name,
        &form.color,
        &scopes,
    )
    .map_err(|error| {
        eprintln!("Unable to update tag: {error}");
        StatusCode::BAD_REQUEST
    })?;
    println!("Tag updated: slug={}", form.slug);
    Ok(Redirect::to("/tags?saved=true"))
}

fn tag_form_scopes(form: &TagForm) -> Vec<database::inventory::TagScope> {
    let mut scopes = Vec::new();
    if form.directory.is_some() {
        scopes.push(database::inventory::TagScope::Directory);
    }
    if form.my_drive.is_some() {
        scopes.push(database::inventory::TagScope::MyDrive);
    }
    if form.shared_drives.is_some() {
        scopes.push(database::inventory::TagScope::SharedDrives);
    }
    if form.shared_with_me.is_some() {
        scopes.push(database::inventory::TagScope::SharedWithMe);
    }
    scopes
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

    let setup_settings = state
        .database()
        .ok()
        .and_then(|database| database::settings::load(&database).ok())
        .unwrap_or_default();
    let directory_setup_decided = setup_settings.directory_sheet_enabled
        || state
            .database()
            .ok()
            .and_then(|database| database::settings::directory_setup_skipped(&database).ok())
            .unwrap_or(false);
    let (setup_steps, setup_percent) = build_setup_progress(
        &rclone_state,
        &google_client_state,
        &google_remotes_state,
        directory_setup_decided,
    );

    let template = SetupProgressTemplate {
        setup_steps,
        setup_percent,
        poll_rclone: should_poll_setup(&rclone_state, &google_remotes_state),
        directory_sheet_enabled: setup_settings.directory_sheet_enabled,
        directory_sheet_url: setup_settings.directory_sheet_url,
    };

    render_template(&template)
}

async fn ui_drive_summaries(
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, StatusCode> {
    let metadata_state = state.metadata_state();
    let shared_summary = latest_shared_summary(&state);
    let template = DriveSummariesTemplate {
        metadata: build_metadata_view(
            &metadata_state,
            true,
            false,
            latest_my_drive_summary(&state).as_ref(),
            shared_summary.as_ref(),
            latest_shared_drives_summary(&state).as_ref(),
            shared_drive_count(&state),
        ),
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
            latest_my_drive_summary(&state).as_ref(),
            shared_summary.as_ref(),
            latest_shared_drives_summary(&state).as_ref(),
            shared_drive_count(&state),
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
    let scopes = metadata_scope_progress_views(&state, &metadata_state);

    render_template(&MetadataUpdateModalTemplate {
        metadata: build_metadata_view(
            &metadata_state,
            available,
            should_poll_setup(&rclone_state, &remotes),
            latest_my_drive_summary(&state).as_ref(),
            shared_summary.as_ref(),
            latest_shared_drives_summary(&state).as_ref(),
            shared_drive_count(&state),
        ),
        scopes,
        directory_available: state
            .database()
            .ok()
            .and_then(|database| database::settings::load(&database).ok())
            .map(|settings| {
                settings.directory_sheet_enabled && !settings.directory_sheet_url.is_empty()
            })
            .unwrap_or(false),
    })
}

fn metadata_scope_progress_views(
    state: &AppState,
    metadata_state: &MetadataState,
) -> Vec<MetadataScopeProgressView> {
    let MetadataState::Updating(progress) = metadata_state else {
        return Vec::new();
    };
    let phase = progress.phase.as_str();
    let selection = progress.selection;
    let shared_drive_percent = if let Some(rest) = phase.strip_prefix("Scanning Shared Drive ") {
        rest.split_once(" of ")
            .and_then(|(current, rest)| {
                let total = rest.split(':').next()?.trim().parse::<u64>().ok()?;
                let current = current.trim().parse::<u64>().ok()?;
                (total > 0).then_some((10 + current.saturating_mul(80) / total).min(90) as u8)
            })
            .unwrap_or(45)
    } else {
        20
    };

    let directory = if matches!(
        phase,
        "Downloading directory spreadsheet" | "Importing directory spreadsheet"
    ) {
        (
            true,
            false,
            if phase.starts_with("Downloading") {
                40
            } else {
                80
            },
            phase.to_string(),
        )
    } else if phase == "Connecting" {
        (false, false, 0, "Waiting".to_string())
    } else {
        (false, true, 100, "Complete".to_string())
    };
    let my_drive = match phase {
        "Fetching My Drive metadata" => (true, false, 40, phase.to_string()),
        "Saving My Drive metadata" => (true, false, 85, phase.to_string()),
        "Saving Shared with me metadata" => (false, true, 100, "Complete".to_string()),
        "Connecting" | "Downloading directory spreadsheet" | "Importing directory spreadsheet" => {
            (false, false, 0, "Waiting".to_string())
        }
        _ => (false, false, 65, "Downloaded; waiting to index".to_string()),
    };
    let shared_drives =
        if phase == "Discovering Shared Drives" || phase.starts_with("Scanning Shared Drive ") {
            (true, false, shared_drive_percent, phase.to_string())
        } else if matches!(
            phase,
            "Fetching Shared with me metadata"
                | "Saving My Drive metadata"
                | "Saving Shared with me metadata"
        ) {
            (false, true, 100, "Complete".to_string())
        } else {
            (false, false, 0, "Waiting".to_string())
        };
    let shared_with_me = match phase {
        "Fetching Shared with me metadata" => (true, false, 45, phase.to_string()),
        "Saving My Drive metadata" => {
            (false, false, 65, "Downloaded; waiting to index".to_string())
        }
        "Saving Shared with me metadata" => (true, false, 85, phase.to_string()),
        _ => (false, false, 0, "Waiting".to_string()),
    };

    [
        ("My Drive", selection.my_drive, "my-drive", my_drive),
        (
            "Shared Drives",
            selection.shared_drives,
            "shared-drives",
            shared_drives,
        ),
        (
            "Shared with me",
            selection.shared_with_me,
            "shared-with-me",
            shared_with_me,
        ),
        ("Persons", selection.directory_info, "", directory),
    ]
    .into_iter()
    .map(
        |(name, selected, scan_type, (mut active, mut complete, mut percent, mut status))| {
            if !selected {
                active = false;
                complete = false;
                percent = 0;
                status = "Not requested".to_string();
            }
            let timing = selected
                .then(|| state.database().ok())
                .flatten()
                .and_then(|database| {
                    (!scan_type.is_empty())
                        .then(|| {
                            database::inventory::scan_timing_estimate(&database, scan_type)
                                .ok()
                                .flatten()
                        })
                        .flatten()
                });
            if active {
                if let Some(timing) = timing.as_ref() {
                    let time_percent = (timing.elapsed_seconds.saturating_mul(100)
                        / timing.average_seconds.max(1))
                    .min(95) as u8;
                    percent = percent.max(time_percent);
                }
            }
            MetadataScopeProgressView {
                name,
                selected,
                active,
                complete,
                status,
                percent,
                elapsed_label: timing
                    .as_ref()
                    .map(|value| format!("{} elapsed", format_duration(value.elapsed_seconds)))
                    .unwrap_or_else(|| {
                        if active {
                            "Timing this update…".to_string()
                        } else {
                            String::new()
                        }
                    }),
                estimate_label: timing
                    .as_ref()
                    .map(|value| {
                        format!(
                            "about {} average from {} update{}",
                            format_duration(value.average_seconds),
                            value.sample_count,
                            if value.sample_count == 1 { "" } else { "s" }
                        )
                    })
                    .unwrap_or_default(),
            }
        },
    )
    .collect()
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

fn latest_my_drive_summary(state: &AppState) -> Option<database::inventory::InventorySummary> {
    let database = state.database().ok()?;
    database::inventory::latest_summary(&database)
        .ok()
        .flatten()
}

fn latest_shared_drives_summary(state: &AppState) -> Option<database::inventory::InventorySummary> {
    let database = state.database().ok()?;
    database::inventory::latest_summary_for(&database, "shared-drives")
        .ok()
        .flatten()
}

fn shared_drive_count(state: &AppState) -> usize {
    state
        .database()
        .ok()
        .and_then(|database| database::inventory::list_shared_drives(&database).ok())
        .map(|drives| {
            drives
                .into_iter()
                .filter(|drive| drive.is_accessible)
                .count()
        })
        .unwrap_or(0)
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

async fn save_setup_directory(
    State(state): State<Arc<AppState>>,
    Form(form): Form<SetupDirectoryForm>,
) -> Result<Redirect, StatusCode> {
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let mut settings =
        database::settings::load(&database).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if form.skip.is_some() {
        database::settings::set_directory_setup_skipped(&database, true)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        log::info!("Optional directory spreadsheet setup skipped");
        return Ok(Redirect::to("/"));
    }
    let url = form.directory_sheet_url.trim();
    if !url.is_empty() {
        crate::rclone::identity::parse_google_sheet_url(url)
            .map_err(|_| StatusCode::BAD_REQUEST)?;
    }
    settings.directory_sheet_enabled = !url.is_empty();
    settings.directory_sheet_url = url.to_string();
    settings.automatic_updates = false;
    settings.update_when_overdue_at_startup = false;
    database::settings::save(&database, &settings)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    database::settings::set_directory_setup_skipped(&database, false)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    log::info!(
        "Optional setup directory URL {}",
        if url.is_empty() { "cleared" } else { "saved" }
    );
    Ok(Redirect::to("/"))
}

fn start_remote_setup(state: Arc<AppState>, kind: RemoteKind) -> Result<Redirect, StatusCode> {
    AppState::configure_google_remote(state, kind).map_err(|error| {
        eprintln!("Unable to start {} setup: {error}", kind.label());
        StatusCode::CONFLICT
    })?;

    Ok(Redirect::to("/"))
}

async fn start_metadata_update(
    State(state): State<Arc<AppState>>,
    Form(form): Form<MetadataUpdateForm>,
) -> Result<Redirect, StatusCode> {
    let remotes = state.google_remotes_state();

    if !matches!(remotes.ro, RemoteState::Ready) {
        return Err(StatusCode::PRECONDITION_FAILED);
    }

    let selection = crate::app::MetadataUpdateSelection {
        my_drive: form.my_drive.is_some(),
        shared_drives: form.shared_drives.is_some(),
        shared_with_me: form.shared_with_me.is_some(),
        directory_info: form.directory_info.is_some(),
    };
    AppState::start_metadata_update(state, selection).map_err(|error| {
        eprintln!("Unable to start metadata update: {error}");
        StatusCode::CONFLICT
    })?;

    Ok(Redirect::to("/"))
}

fn should_poll_rclone(rclone_state: &RcloneState) -> bool {
    matches!(rclone_state, RcloneState::Initializing)
}

fn should_poll_setup(rclone_state: &RcloneState, remotes_state: &GoogleRemotesState) -> bool {
    should_poll_rclone(rclone_state) || matches!(remotes_state.ro, RemoteState::Configuring)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::MetadataProgress;

    #[test]
    fn active_scope_progress_does_not_replace_my_drive_summary() {
        let my_drive = database::inventory::InventorySummary {
            completed_at: "2026-08-30 12:00:00".to_string(),
            files_scanned: 10,
            folders_scanned: 2,
            permissions_scanned: 15,
            bytes_discovered: 1_000,
            deleted_items: 0,
        };
        let state = MetadataState::Updating(MetadataProgress {
            selection: crate::app::MetadataUpdateSelection {
                my_drive: false,
                shared_drives: true,
                shared_with_me: false,
                directory_info: false,
            },
            phase: "Scanning Shared Drive 1 of 1: Research".to_string(),
            files_scanned: 500,
            folders_scanned: 75,
            permissions_scanned: 900,
            bytes_discovered: 8_000_000,
            errors: 0,
        });

        let view = build_metadata_view(&state, true, false, Some(&my_drive), None, None, 1);

        assert_eq!(view.files_scanned, 10);
        assert_eq!(view.folders_scanned, 2);
        assert_eq!(view.permissions_scanned, 15);
        assert_eq!(view.progress_files_scanned, 500);
        assert_eq!(view.progress_folders_scanned, 75);
        assert_eq!(view.progress_permissions_scanned, 900);
    }

    #[test]
    fn download_names_are_safe_local_path_components() {
        assert_eq!(
            safe_download_name("Budget: 2026/Final?.xlsx"),
            "Budget_ 2026_Final_.xlsx"
        );
        assert_eq!(safe_download_name(".."), "Drive item");
        assert_eq!(safe_download_name("CON.txt"), "_CON.txt");
        assert_eq!(safe_download_name("Research"), "Research");
    }
}
