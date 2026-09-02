use std::{
    path::{Path as FsPath, PathBuf},
    process::Command,
    sync::{Arc, OnceLock},
};

use askama::Template;

use axum::{
    Router,
    body::Body,
    extract::{Form, Multipart, Path, Query, Request, State},
    http::{Response, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};

use crate::{
    app::{
        AppState, DownloadState, GoogleClientState, GoogleRemotesState, MetadataState, RcloneState,
    },
    config,
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

use super::xlsx;

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
    pub dismiss_action: &'static str,
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
    pub title: String,
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
    pub action: &'static str,
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
    initial_setup_complete: bool,
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
    boreal_url: String,
    bookmark_reminder_dismissed: bool,
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
#[derive(Template)]
#[template(path = "update.html", config = "askama.toml")]
struct UpdateTemplate {
    title: &'static str,
    active_page: &'static str,
    alerts: Vec<AlertItem>,
    status_items: Vec<StatusItem>,
    poll_rclone: bool,
    current_version: String,
    current_maturity: String,
    checking: bool,
    update_available: bool,
    error: String,
    latest_version: String,
    latest_maturity: String,
    latest_date: String,
    summary: String,
    notes: String,
    release_url: String,
    download_url: String,
    changelog_url: &'static str,
}

#[allow(dead_code)]
#[derive(Template)]
#[template(path = "docs.html", config = "askama.toml")]
struct DocsTemplate {
    title: &'static str,
    active_page: &'static str,
    alerts: Vec<AlertItem>,
    status_items: Vec<StatusItem>,
    poll_rclone: bool,
}

#[allow(dead_code)]
#[derive(Template)]
#[template(path = "help.html", config = "askama.toml")]
struct HelpTemplate {
    title: &'static str,
    active_page: &'static str,
    alerts: Vec<AlertItem>,
    status_items: Vec<StatusItem>,
    poll_rclone: bool,
}

#[allow(dead_code)]
pub struct MigrationView {
    pub id: i64,
    pub source_label: String,
    pub operation_label: String,
    pub destination_kind: String,
    pub destination_path: String,
    pub can_open_local: bool,
    pub status: String,
    pub phase: String,
    pub destination_url: String,
    pub destination_label: String,
    pub files_total: u64,
    pub folders_total: u64,
    pub size_label: String,
    pub files_copied: u64,
    pub copied_size_label: String,
    pub exceptions_count: u64,
    pub created_at: String,
    pub started_at: String,
    pub completed_at: String,
    pub copy_completed_at: String,
    pub resume_count: u64,
    pub error_message: String,
    pub archived_at: String,
    pub can_cancel: bool,
    pub can_archive: bool,
    pub can_start: bool,
    pub can_resume: bool,
    pub running: bool,
    pub allows_my_drive_destination: bool,
    pub allows_google_destination: bool,
    pub sources: Vec<MigrationSourceView>,
}

#[allow(dead_code)]
pub struct MigrationSourceView {
    pub item_id: String,
    pub name: String,
    pub relative_path: String,
    pub is_directory: bool,
    pub files_total: u64,
    pub folders_total: u64,
    pub bytes_total: u64,
    pub status: String,
    pub error_message: String,
    pub drive_url: String,
}

#[allow(dead_code)]
pub struct MigrationSortHeader {
    pub label: &'static str,
    pub url: String,
    pub active: bool,
    pub descending: bool,
}

#[allow(dead_code)]
#[derive(Template)]
#[template(path = "migrations.html", config = "askama.toml")]
struct MigrationsTemplate {
    title: &'static str,
    active_page: &'static str,
    alerts: Vec<AlertItem>,
    status_items: Vec<StatusItem>,
    poll_rclone: bool,
    migrations: Vec<MigrationView>,
    search: String,
    include_archived: bool,
    sort_headers: Vec<MigrationSortHeader>,
}

#[allow(dead_code)]
#[derive(Template)]
#[template(path = "migration-wizard.html", config = "askama.toml")]
struct MigrationWizardTemplate {
    title: &'static str,
    active_page: &'static str,
    alerts: Vec<AlertItem>,
    status_items: Vec<StatusItem>,
    poll_rclone: bool,
    migration: MigrationView,
    error: String,
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
    my_drive_ro_configured: bool,
    my_drive_rw_configured: bool,
    remote_setup_busy: bool,
    google_client_ready: bool,
    rclone_ready: bool,
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
    pub description: String,
    pub color: String,
    pub text_color: &'static str,
}

#[allow(dead_code)]
pub struct TagFilterPill {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub color: String,
    pub text_color: &'static str,
    pub selected: bool,
}

#[allow(dead_code)]
pub struct IdentityTagFilterPill {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub color: String,
    pub text_color: &'static str,
    pub owner_selected: bool,
    pub permission_selected: bool,
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
    filter_tags: Vec<TagFilterPill>,
    identity_filter_tags: Vec<IdentityTagFilterPill>,
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
    root_label: String,
    explorer_path: String,
    export_path: &'static str,
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

#[derive(Template)]
#[template(path = "partials/download-status-item.html", config = "askama.toml")]
struct DownloadStatusItemTemplate {
    item: StatusItem,
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
    pub modified_at: String,
    pub tags: Vec<TagPill>,
}

pub struct SharedDriveIdentityView {
    pub label: String,
    pub roles_label: String,
    pub tagged: bool,
    pub unknown: bool,
    pub color: String,
    pub text_color: &'static str,
    pub tag_details: String,
    pub directory_url: String,
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
    filter_tags: Vec<TagFilterPill>,
    search: String,
    tag_filter: String,
    tagged_count: usize,
    untagged_count: usize,
    files_filter: String,
    folders_filter: String,
    size_filter: String,
    modified_filter: String,
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
    initial_setup_complete: bool,
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
    modified_filter: String,
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
    #[serde(default)]
    description: String,
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
struct DeleteTagForm {
    slug: String,
}

#[derive(serde::Deserialize)]
struct SettingsForm {
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

#[derive(serde::Deserialize)]
struct SelectedMetadataUpdateForm {
    #[serde(default)]
    inventory_scope: String,
    #[serde(default)]
    selected_item_ids: String,
    #[serde(default)]
    selected_drive_ids: String,
    #[serde(default)]
    drive: String,
}

#[derive(serde::Deserialize)]
struct NewMigrationForm {
    #[serde(default)]
    selected_item_ids: String,
    #[serde(default)]
    inventory_scope: String,
    #[serde(default)]
    intent: String,
}

#[derive(serde::Deserialize)]
struct DownloadMigrationForm {
    item_id: String,
    inventory_scope: String,
}

#[derive(serde::Deserialize)]
struct MigrationDestinationForm {
    destination_url: String,
}

#[derive(serde::Deserialize)]
struct AddRemoteForm {
    remote_kind: String,
}

#[derive(Default, serde::Deserialize)]
struct MigrationListQuery {
    #[serde(default)]
    q: String,
    #[serde(default)]
    archived: Option<String>,
    #[serde(default)]
    sort: String,
    #[serde(default)]
    dir: String,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(index))
        .route("/about", get(about))
        .route("/update", get(update_page).post(check_for_updates))
        .route("/docs", get(docs_page))
        .route("/help", get(help_page))
        .route("/assets/uaf-logo.png", get(uaf_logo))
        .route("/assets/acep-logo.png", get(acep_logo))
        .route(
            "/assets/google-cloud-project-selection.png",
            get(google_cloud_project_selection),
        )
        .route(
            "/assets/google-cloud-enable-api.png",
            get(google_cloud_enable_api),
        )
        .route(
            "/assets/google-cloud-create-client.png",
            get(google_cloud_create_client),
        )
        .route(
            "/assets/google-cloud-oauth-json.png",
            get(google_cloud_oauth_json),
        )
        .route("/remotes", get(remotes_page))
        .route("/remotes/add", post(add_remote))
        .route("/my-drive", get(my_drive_page))
        .route("/my-drive/export.xlsx", get(export_my_drive))
        .route("/my-drive/tags", post(apply_my_drive_tag))
        .route("/my-drive/tags/remove", post(remove_my_drive_tag))
        .route("/shared-drives", get(shared_drives_page))
        .route("/shared-drives/export.xlsx", get(export_shared_drives))
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
        .route("/shared-with-me/export.xlsx", get(export_shared_with_me))
        .route("/shared-with-me/tags", post(apply_shared_with_me_tag))
        .route(
            "/shared-with-me/tags/remove",
            post(remove_shared_with_me_tag),
        )
        .route("/migrations", get(migrations_page))
        .route("/migrations/new", post(create_migration))
        .route("/migrations/download", post(create_download_migration))
        .route(
            "/migrations/download/shared-drive/{drive_id}",
            post(create_shared_drive_download_migration),
        )
        .route("/migrations/{migration_id}", get(migration_wizard))
        .route("/migrations/{migration_id}/cancel", post(cancel_migration))
        .route(
            "/migrations/{migration_id}/archive",
            post(archive_migration),
        )
        .route(
            "/migrations/{migration_id}/destination",
            post(save_migration_destination),
        )
        .route(
            "/migrations/{migration_id}/local-destination",
            post(save_local_migration_destination),
        )
        .route(
            "/migrations/{migration_id}/open-local",
            post(open_local_migration_destination),
        )
        .route(
            "/migrations/{migration_id}/start",
            post(start_migration_copy),
        )
        .route("/ui/download-status", get(ui_download_status))
        .route("/ui/download-status-item", get(ui_download_status_item))
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
        .route("/directory/template.csv", get(directory_csv_template))
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
        .route("/tags/delete", post(delete_tag))
        .route("/settings", get(settings_page).post(save_settings))
        .route(
            "/settings/bookmark-reminder/dismiss",
            post(dismiss_bookmark_reminder),
        )
        .route(
            "/settings/bookmark-reminder/show",
            post(show_bookmark_reminder),
        )
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
        .route("/setup/metadata/skip", post(skip_setup_metadata))
        .route("/metadata/update", post(start_metadata_update))
        .route(
            "/metadata/update-selected",
            post(start_selected_metadata_update),
        )
        .route("/app/quit", post(quit))
        .layer(middleware::from_fn(log_http_request))
}

async fn log_http_request(request: Request, next: Next) -> axum::response::Response {
    let method = request.method().clone();
    let uri = request.uri().clone();
    let started = std::time::Instant::now();
    let response = next.run(request).await;
    let status = response.status();
    let elapsed_ms = started.elapsed().as_millis();
    if method == axum::http::Method::GET {
        log::debug!("HTTP {method} {uri} -> {status} ({elapsed_ms} ms)");
    } else {
        log::info!("HTTP {method} {uri} -> {status} ({elapsed_ms} ms)");
    }
    response
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

async fn google_cloud_project_selection() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        include_bytes!("../../tmpl/html/img/GC_Project_Selection.png").as_slice(),
    )
}

async fn google_cloud_enable_api() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        include_bytes!("../../tmpl/html/img/GC_EnableAPIAccess.png").as_slice(),
    )
}

async fn google_cloud_create_client() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        include_bytes!("../../tmpl/html/img/GC_Create_Client.png").as_slice(),
    )
}

async fn google_cloud_oauth_json() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        include_bytes!("../../tmpl/html/img/GC_OAuth_JSON_redacted.png").as_slice(),
    )
}

const PERSONS_CSV_TEMPLATE: &str = "name,email,organization,type,status,departure_date,notes\r\n";

async fn directory_csv_template() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"boreal-persons-template.csv\"",
            ),
        ],
        PERSONS_CSV_TEMPLATE,
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

    let alerts = build_alerts(
        &rclone_state,
        &google_client_state,
        bookmark_reminder_visible(&state),
    );

    let status_items = build_status_items(
        &state.download_state(),
        &rclone_state,
        &google_client_state,
        &google_remotes_state,
        &metadata_state,
        configured_remote_count(&state.runtime, &rclone_state),
        authenticated_google_email(&state),
        &state.update_state(),
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
    let initial_setup_complete = setup_percent == 100 && metadata_setup_decided(&state);

    let poll_rclone = should_poll_ui(&rclone_state, &google_remotes_state, &metadata_state);

    let shared_summary = latest_shared_summary(&state);
    let template = DashboardTemplate {
        title: "BOREAL",
        active_page: "dashboard",
        alerts,
        status_items,
        setup_steps,
        setup_percent,
        initial_setup_complete,
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
            action: "",
            disabled: true,
            detail: String::new(),
        },

        RcloneState::Ready(status) => SetupStep {
            icon: "bi-check-circle-fill",
            title: "Install Rclone",
            description: format!("{} is installed and ready.", status.version),
            state_label: "Complete",
            state_class: "text-bg-success",
            complete: true,
            modal_target: "",
            action: "",
            disabled: true,
            detail: String::new(),
        },

        RcloneState::Error(error) => SetupStep {
            icon: "bi-exclamation-triangle-fill",
            title: "Install Rclone",
            description: format!("Rclone setup failed: {error}"),
            state_label: "Needs attention",
            state_class: "text-bg-danger",
            complete: false,
            modal_target: "",
            action: "",
            disabled: true,
            detail: String::new(),
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
            action: "",
            disabled: false,
            detail: String::new(),
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
            action: "",
            disabled: true,
            detail: String::new(),
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
            action: "",
            disabled: false,
            detail: String::new(),
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
        action: if remote_complete {
            ""
        } else {
            "/setup/remotes/my-drive-ro"
        },
        disabled: !prerequisites_ready
            || remote_busy
            || matches!(
                google_remotes_state.ro,
                RemoteState::Ready | RemoteState::Conflict(_)
            ),
        detail: match &google_remotes_state.ro {
            RemoteState::Conflict(error) | RemoteState::Error(error) => error.clone(),
            RemoteState::Configuring => {
                "Complete the Google authorization in the browser tab opened by Rclone."
                    .to_string()
            }
            _ => String::new(),
        },
    };

    let steps = vec![rclone_step, google_step, remote_step];

    let complete_count =
        steps.iter().filter(|step| step.complete).count() + usize::from(directory_setup_decided);
    let setup_percent = (complete_count * 100 / 4) as u8;

    (steps, setup_percent)
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
    const TB: f64 = 1_000_000_000_000.0;
    const GB: f64 = 1_000_000_000.0;
    const MB: f64 = 1_000_000.0;
    const KB: f64 = 1_000.0;

    if bytes as f64 >= TB {
        format!("{:.1} TB", bytes as f64 / TB,)
    } else if bytes as f64 >= GB {
        format!("{:.1} GB", bytes as f64 / GB,)
    } else if bytes as f64 >= MB {
        format!("{:.1} MB", bytes as f64 / MB,)
    } else if bytes as f64 >= KB {
        format!("{:.1} KB", bytes as f64 / KB,)
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
    let directory_sheet_url = form.directory_sheet_url.trim().to_string();
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let mut inventory_settings =
        settings::load(&database).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    inventory_settings.directory_sheet_enabled = !directory_sheet_url.is_empty();
    inventory_settings.directory_sheet_url = directory_sheet_url;
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

async fn dismiss_bookmark_reminder(
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, StatusCode> {
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    settings::set_bookmark_reminder_dismissed(&database, true)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    ui_alerts(State(state)).await
}

async fn show_bookmark_reminder(
    State(state): State<Arc<AppState>>,
) -> Result<Redirect, StatusCode> {
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    settings::set_bookmark_reminder_dismissed(&database, false)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Redirect::to("/settings?saved=true#bookmark-this-page"))
}

async fn test_directory_sheet(
    State(state): State<Arc<AppState>>,
    Form(form): Form<SettingsForm>,
) -> Result<axum::response::Response, StatusCode> {
    let directory_sheet_url = form.directory_sheet_url.trim().to_string();
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let mut inventory_settings =
        settings::load(&database).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    inventory_settings.directory_sheet_enabled = !directory_sheet_url.is_empty();
    inventory_settings.directory_sheet_url = directory_sheet_url;
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
        alerts: build_alerts(
            &rclone_state,
            &google_client_state,
            bookmark_reminder_visible(state),
        ),
        status_items: build_status_items(
            &state.download_state(),
            &rclone_state,
            &google_client_state,
            &google_remotes_state,
            &metadata_state,
            configured_remote_count(&state.runtime, &rclone_state),
            authenticated_google_email(&state),
            &state.update_state(),
        ),
        poll_rclone: should_poll_ui(&rclone_state, &google_remotes_state, &metadata_state),
        settings: inventory_settings,
        saved,
        error,
        notice,
        directory_source,
        boreal_url: boreal_web_url(state),
        bookmark_reminder_dismissed: !bookmark_reminder_visible(state),
    };

    render_template(&template)
}

fn bookmark_reminder_visible(state: &AppState) -> bool {
    state
        .database()
        .ok()
        .and_then(|database| settings::bookmark_reminder_dismissed(&database).ok())
        .map(|dismissed| !dismissed)
        .unwrap_or(true)
}

fn boreal_web_url(state: &AppState) -> String {
    config::get_webapp_config(&state.runtime.boreal)
        .map(|webapp| {
            let host = if webapp.listen == "::1" {
                "[::1]".to_string()
            } else {
                webapp.listen
            };
            format!("http://{host}:{}", webapp.port)
        })
        .unwrap_or_else(|_| "http://127.0.0.1:8765".to_string())
}

async fn about(State(state): State<Arc<AppState>>) -> Result<Html<String>, StatusCode> {
    let rclone_state = state.rclone_state();

    let google_client_state = state.google_client_state();
    let google_remotes_state = state.google_remotes_state();
    let metadata_state = state.metadata_state();

    let alerts = build_alerts(
        &rclone_state,
        &google_client_state,
        bookmark_reminder_visible(&state),
    );

    let status_items = build_status_items(
        &state.download_state(),
        &rclone_state,
        &google_client_state,
        &google_remotes_state,
        &metadata_state,
        configured_remote_count(&state.runtime, &rclone_state),
        authenticated_google_email(&state),
        &state.update_state(),
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

async fn update_page(State(state): State<Arc<AppState>>) -> Result<Html<String>, StatusCode> {
    let rclone_state = state.rclone_state();
    let google_client_state = state.google_client_state();
    let google_remotes_state = state.google_remotes_state();
    let metadata_state = state.metadata_state();
    let update_state = state.update_state();
    let (checking, update_available, error, release) = match &update_state {
        crate::update::UpdateState::Checking => (true, false, String::new(), None),
        crate::update::UpdateState::Current { latest } => {
            (false, false, String::new(), Some(latest))
        }
        crate::update::UpdateState::Available { release } => {
            (false, true, String::new(), Some(release))
        }
        crate::update::UpdateState::Error(error) => (false, false, error.clone(), None),
    };
    let latest_version = release
        .map(|release| release.version.clone())
        .unwrap_or_default();
    render_template(&UpdateTemplate {
        title: "Update BOREAL",
        active_page: "update",
        alerts: build_alerts(
            &rclone_state,
            &google_client_state,
            bookmark_reminder_visible(&state),
        ),
        status_items: build_status_items(
            &state.download_state(),
            &rclone_state,
            &google_client_state,
            &google_remotes_state,
            &metadata_state,
            configured_remote_count(&state.runtime, &rclone_state),
            authenticated_google_email(&state),
            &update_state,
        ),
        poll_rclone: should_poll_ui(&rclone_state, &google_remotes_state, &metadata_state)
            || checking,
        current_version: env!("CARGO_PKG_VERSION").to_string(),
        current_maturity: boreal_maturity().to_string(),
        checking,
        update_available,
        error,
        latest_version: latest_version.clone(),
        latest_maturity: release
            .map(|release| release.maturity.clone())
            .unwrap_or_default(),
        latest_date: release
            .map(|release| release.date.clone())
            .unwrap_or_default(),
        summary: release
            .map(|release| release.summary.clone())
            .unwrap_or_default(),
        notes: release
            .map(|release| release.notes.clone())
            .unwrap_or_default(),
        release_url: (!latest_version.is_empty())
            .then(|| crate::update::release_url(&latest_version))
            .unwrap_or_default(),
        download_url: (!latest_version.is_empty())
            .then(|| crate::update::download_url(&latest_version))
            .flatten()
            .unwrap_or_default(),
        changelog_url: crate::update::CHANGELOG_URL,
    })
}

async fn check_for_updates(State(state): State<Arc<AppState>>) -> Redirect {
    AppState::check_for_updates(state);
    Redirect::to("/update")
}

async fn docs_page(State(state): State<Arc<AppState>>) -> Result<Html<String>, StatusCode> {
    let rclone_state = state.rclone_state();
    let google_client_state = state.google_client_state();
    let google_remotes_state = state.google_remotes_state();
    let metadata_state = state.metadata_state();
    render_template(&DocsTemplate {
        title: "BOREAL Docs",
        active_page: "docs",
        alerts: build_alerts(
            &rclone_state,
            &google_client_state,
            bookmark_reminder_visible(&state),
        ),
        status_items: build_status_items(
            &state.download_state(),
            &rclone_state,
            &google_client_state,
            &google_remotes_state,
            &metadata_state,
            configured_remote_count(&state.runtime, &rclone_state),
            authenticated_google_email(&state),
            &state.update_state(),
        ),
        poll_rclone: should_poll_ui(&rclone_state, &google_remotes_state, &metadata_state),
    })
}

async fn help_page(State(state): State<Arc<AppState>>) -> Result<Html<String>, StatusCode> {
    let rclone_state = state.rclone_state();
    let google_client_state = state.google_client_state();
    let google_remotes_state = state.google_remotes_state();
    let metadata_state = state.metadata_state();
    render_template(&HelpTemplate {
        title: "BOREAL Help",
        active_page: "help",
        alerts: build_alerts(
            &rclone_state,
            &google_client_state,
            bookmark_reminder_visible(&state),
        ),
        status_items: build_status_items(
            &state.download_state(),
            &rclone_state,
            &google_client_state,
            &google_remotes_state,
            &metadata_state,
            configured_remote_count(&state.runtime, &rclone_state),
            authenticated_google_email(&state),
            &state.update_state(),
        ),
        poll_rclone: should_poll_ui(&rclone_state, &google_remotes_state, &metadata_state),
    })
}

async fn migrations_page(
    State(state): State<Arc<AppState>>,
    Query(query): Query<MigrationListQuery>,
) -> Result<Html<String>, StatusCode> {
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let sort = match query.sort.as_str() {
        "id" | "source" | "status" | "destination" | "files" | "folders" | "capacity"
        | "created" => query.sort.as_str(),
        _ => "created",
    };
    let descending = if query.dir.is_empty() {
        true
    } else {
        query.dir == "desc"
    };
    let include_archived = query.archived.is_some();
    let migrations =
        database::migration::list(&database, &query.q, include_archived, sort, descending)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .into_iter()
            .map(migration_view)
            .collect();
    let sort_headers = [
        ("ID", "id"),
        ("Source", "source"),
        ("Status / Phase", "status"),
        ("Destination", "destination"),
        ("Files", "files"),
        ("Folders", "folders"),
        ("Capacity", "capacity"),
        ("Created", "created"),
    ]
    .into_iter()
    .map(|(label, column)| {
        let active = sort == column;
        let next_descending = if active { !descending } else { false };
        MigrationSortHeader {
            label,
            url: migration_list_url(&query.q, include_archived, column, next_descending),
            active,
            descending,
        }
    })
    .collect();
    let rclone_state = state.rclone_state();
    let google_client_state = state.google_client_state();
    let google_remotes_state = state.google_remotes_state();
    let metadata_state = state.metadata_state();
    render_template(&MigrationsTemplate {
        title: "Migrations - BOREAL",
        active_page: "migrations",
        alerts: build_alerts(
            &rclone_state,
            &google_client_state,
            bookmark_reminder_visible(&state),
        ),
        status_items: build_status_items(
            &state.download_state(),
            &rclone_state,
            &google_client_state,
            &google_remotes_state,
            &metadata_state,
            configured_remote_count(&state.runtime, &rclone_state),
            authenticated_google_email(&state),
            &state.update_state(),
        ),
        poll_rclone: should_poll_ui(&rclone_state, &google_remotes_state, &metadata_state),
        migrations,
        search: query.q,
        include_archived,
        sort_headers,
    })
}

async fn cancel_migration(
    State(state): State<Arc<AppState>>,
    Path(migration_id): Path<i64>,
) -> Result<Redirect, StatusCode> {
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    database::migration::cancel(&database, migration_id).map_err(|error| {
        log::warn!("Unable to cancel migration {migration_id}: {error}");
        StatusCode::CONFLICT
    })?;
    log::info!("Migration canceled and removed: migration_id={migration_id}");
    Ok(Redirect::to("/migrations"))
}

async fn archive_migration(
    State(state): State<Arc<AppState>>,
    Path(migration_id): Path<i64>,
) -> Result<Redirect, StatusCode> {
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    database::migration::archive(&database, migration_id).map_err(|error| {
        log::warn!("Unable to archive migration {migration_id}: {error}");
        StatusCode::CONFLICT
    })?;
    log::info!("Migration archived: migration_id={migration_id}");
    Ok(Redirect::to("/migrations"))
}

fn migration_list_url(
    search: &str,
    include_archived: bool,
    sort: &str,
    descending: bool,
) -> String {
    format!(
        "/migrations?q={}&sort={sort}&dir={}{}",
        url_encode_component(search),
        if descending { "desc" } else { "asc" },
        if include_archived { "&archived=1" } else { "" },
    )
}

fn url_encode_component(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

async fn create_migration(
    State(state): State<Arc<AppState>>,
    Form(form): Form<NewMigrationForm>,
) -> Result<Redirect, StatusCode> {
    let source_kind = match form.inventory_scope.as_str() {
        database::inventory::MY_DRIVE_SCOPE => "my-drive",
        database::inventory::SHARED_WITH_ME_SCOPE => "shared-with-me",
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    let item_ids = form
        .selected_item_ids
        .split(',')
        .map(str::trim)
        .filter(|item_id| !item_id.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let operation_kind = if form.intent == "local-download" {
        "local-download"
    } else {
        "drive-copy"
    };
    let id = database::migration::create(
        &database,
        &form.inventory_scope,
        source_kind,
        &item_ids,
        operation_kind,
    )
    .map_err(|error| {
        eprintln!("Unable to create migration plan: {error}");
        StatusCode::BAD_REQUEST
    })?;
    log::info!(
        "Migration plan created: migration_id={id}, source_kind={source_kind}, sources={}",
        item_ids.len(),
    );
    Ok(Redirect::to(&format!("/migrations/{id}")))
}

async fn create_download_migration(
    State(state): State<Arc<AppState>>,
    Form(form): Form<DownloadMigrationForm>,
) -> Result<Redirect, StatusCode> {
    let source_kind = match form.inventory_scope.as_str() {
        database::inventory::MY_DRIVE_SCOPE => "my-drive",
        database::inventory::SHARED_WITH_ME_SCOPE => "shared-with-me",
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let id = database::migration::create(
        &database,
        &form.inventory_scope,
        source_kind,
        &[form.item_id],
        "local-download",
    )
    .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Redirect::to(&format!("/migrations/{id}")))
}

async fn create_shared_drive_download_migration(
    State(state): State<Arc<AppState>>,
    Path(drive_id): Path<String>,
) -> Result<Redirect, StatusCode> {
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let id = database::migration::create_shared_drive_download(&database, &drive_id)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Redirect::to(&format!("/migrations/{id}")))
}

async fn migration_wizard(
    State(state): State<Arc<AppState>>,
    Path(migration_id): Path<i64>,
) -> Result<Html<String>, StatusCode> {
    render_migration_wizard(&state, migration_id, String::new())
}

async fn save_migration_destination(
    State(state): State<Arc<AppState>>,
    Path(migration_id): Path<i64>,
    Form(form): Form<MigrationDestinationForm>,
) -> Result<axum::response::Response, StatusCode> {
    let result = async {
        let folder_id = google_drive_folder_id(&form.destination_url)?;
        let database = state.database().map_err(|error| error.to_string())?;
        let job = database::migration::get(&database, migration_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("Unknown migration: {migration_id}"))?;
        if job.source_kind == "shared-drive" {
            return Err("Shared Drives can only be copied to a local destination.".to_string());
        }
        let require_shared_drive = job.source_kind == "my-drive";
        let executable = match state.rclone_state() {
            RcloneState::Ready(status) => status.path,
            RcloneState::Initializing => {
                return Err("Rclone is still initializing. Try again when it is ready.".to_string());
            }
            RcloneState::Error(error) => {
                return Err(format!("Rclone is not ready: {error}"));
            }
        };
        let probe_state = Arc::clone(&state);
        let probe_folder_id = folder_id.clone();
        let destination = tokio::task::spawn_blocking(move || {
            rclone::migration::validate_destination(
                &probe_state.runtime,
                &executable,
                &probe_folder_id,
                require_shared_drive,
            )
        })
        .await
        .map_err(|error| format!("Destination validation task failed: {error}"))?
        .map_err(|error| error.to_string())?;

        let local_destination = database::migration::resolve_destination(&database, &folder_id)
            .map_err(|error| error.to_string())?;
        let (drive_name, folder_name) = match local_destination {
            Some((local_drive_id, drive_name, folder_name))
                if local_drive_id == destination.drive_id =>
            {
                (drive_name, folder_name)
            }
            _ => (
                destination.drive_name.clone(),
                destination.folder_name.clone(),
            ),
        };
        let discovered_folders = destination
            .folders
            .iter()
            .filter(|folder| {
                if destination.drive_id.is_empty() {
                    !folder.parents.is_empty()
                } else {
                    folder.id != destination.drive_id
                }
            })
            .map(|folder| database::inventory::DiscoveredDriveFolder {
                item_id: folder.id.clone(),
                name: folder.name.clone(),
                modified_at: folder.modified_at.clone(),
            })
            .collect::<Vec<_>>();
        database::inventory::record_migration_destination(
            &database,
            &destination.drive_id,
            &drive_name,
            &discovered_folders,
        )
        .map_err(|error| error.to_string())?;
        database::migration::set_destination(
            &database,
            migration_id,
            form.destination_url.trim(),
            &destination.drive_id,
            &drive_name,
            &folder_id,
            &folder_name,
        )
        .map_err(|error| error.to_string())
    }
    .await;
    match result {
        Ok(()) => {
            log::info!("Migration destination validated: migration_id={migration_id}");
            Ok(Redirect::to(&format!("/migrations/{migration_id}")).into_response())
        }
        Err(error) => {
            log::warn!(
                "Migration destination validation failed: migration_id={migration_id}, error={error}"
            );
            render_migration_wizard(&state, migration_id, error).map(IntoResponse::into_response)
        }
    }
}

async fn save_local_migration_destination(
    State(state): State<Arc<AppState>>,
    Path(migration_id): Path<i64>,
) -> Result<axum::response::Response, StatusCode> {
    let selected = rfd::FileDialog::new()
        .set_title("Choose BOREAL migration destination")
        .pick_folder();
    let Some(selected) = selected else {
        return render_migration_wizard(
            &state,
            migration_id,
            "Local folder selection was cancelled.".to_string(),
        )
        .map(IntoResponse::into_response);
    };
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    database::migration::set_local_destination(
        &database,
        migration_id,
        &selected.display().to_string(),
    )
    .map_err(|_| StatusCode::CONFLICT)?;
    Ok(Redirect::to(&format!("/migrations/{migration_id}")).into_response())
}

async fn open_local_migration_destination(
    State(state): State<Arc<AppState>>,
    Path(migration_id): Path<i64>,
) -> Result<axum::response::Response, StatusCode> {
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let migration = database::migration::get(&database, migration_id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if migration.destination_kind != "local" || migration.destination_path.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let path = FsPath::new(&migration.destination_path);
    if !path.is_dir() {
        return render_migration_wizard(
            &state,
            migration_id,
            format!(
                "The local destination folder no longer exists: {}",
                migration.destination_path
            ),
        )
        .map(IntoResponse::into_response);
    }
    let open_path = PathBuf::from(path);
    let open_result = tokio::task::spawn_blocking(move || open_folder_in_os(&open_path))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if let Err(error) = open_result {
        log::warn!(
            "Unable to open local migration destination: migration_id={migration_id}, path={}, error={error}",
            migration.destination_path,
        );
        return render_migration_wizard(&state, migration_id, error)
            .map(IntoResponse::into_response);
    }
    log::info!(
        "Opened local migration destination: migration_id={migration_id}, path={}",
        migration.destination_path,
    );
    Ok(StatusCode::NO_CONTENT.into_response())
}

fn open_folder_in_os(path: &FsPath) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let result = Command::new("explorer").arg(path).status();
    #[cfg(target_os = "macos")]
    let result = Command::new("open").arg(path).status();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = Command::new("xdg-open").arg(path).status();
    #[cfg(not(any(target_os = "windows", target_os = "macos", unix)))]
    let result: std::io::Result<std::process::ExitStatus> = Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "No supported file explorer command is available",
    ));

    match result {
        Ok(status) if status.success() => Ok(()),
        result => {
            let command_error = match result {
                Ok(status) => format!("file explorer exited with {status}"),
                Err(error) => error.to_string(),
            };
            let file_url = local_file_url(path)?;
            webbrowser::open(&file_url).map_err(|browser_error| {
                format!(
                    "Unable to open the folder in the OS file explorer ({command_error}) or as a file URL ({browser_error})."
                )
            })
        }
    }
}

fn local_file_url(path: &FsPath) -> Result<String, String> {
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("Unable to resolve the local destination: {error}"))?;
    let normalized = canonical.to_string_lossy().replace('\\', "/");
    let encoded = normalized
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b':' | b'-' | b'.' | b'_' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect::<String>();
    Ok(if encoded.starts_with('/') {
        format!("file://{encoded}")
    } else {
        format!("file:///{encoded}")
    })
}

async fn start_migration_copy(
    State(state): State<Arc<AppState>>,
    Path(migration_id): Path<i64>,
) -> Result<axum::response::Response, StatusCode> {
    match AppState::start_migration_copy(Arc::clone(&state), migration_id) {
        Ok(()) => Ok(Redirect::to(&format!("/migrations/{migration_id}")).into_response()),
        Err(error) => {
            render_migration_wizard(&state, migration_id, error).map(IntoResponse::into_response)
        }
    }
}

fn render_migration_wizard(
    state: &AppState,
    migration_id: i64,
    error: String,
) -> Result<Html<String>, StatusCode> {
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let migration = database::migration::get(&database, migration_id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let rclone_state = state.rclone_state();
    let google_client_state = state.google_client_state();
    let google_remotes_state = state.google_remotes_state();
    let metadata_state = state.metadata_state();
    render_template(&MigrationWizardTemplate {
        title: "Migration Assistant - BOREAL",
        active_page: "migrations",
        alerts: build_alerts(
            &rclone_state,
            &google_client_state,
            bookmark_reminder_visible(state),
        ),
        status_items: build_status_items(
            &state.download_state(),
            &rclone_state,
            &google_client_state,
            &google_remotes_state,
            &metadata_state,
            configured_remote_count(&state.runtime, &rclone_state),
            authenticated_google_email(state),
            &state.update_state(),
        ),
        poll_rclone: should_poll_ui(&rclone_state, &google_remotes_state, &metadata_state),
        migration: migration_view(migration),
        error,
    })
}

fn migration_view(job: database::migration::MigrationJob) -> MigrationView {
    let can_cancel = job.started_at.is_empty() && matches!(job.status.as_str(), "draft" | "ready");
    let can_archive =
        !matches!(job.status.as_str(), "preflight" | "running") && job.archived_at.is_empty();
    let can_start = job.status == "ready" && job.started_at.is_empty();
    let can_resume = matches!(job.status.as_str(), "interrupted" | "error" | "copied")
        && job.archived_at.is_empty();
    let running = matches!(job.status.as_str(), "preflight" | "running");
    let allows_my_drive_destination = job.source_kind == "shared-with-me";
    let allows_google_destination = job.source_kind != "shared-drive";
    let sources = job
        .sources
        .into_iter()
        .map(|source| MigrationSourceView {
            drive_url: if source.is_directory {
                format!("https://drive.google.com/drive/folders/{}", source.item_id)
            } else {
                format!("https://drive.google.com/open?id={}", source.item_id)
            },
            item_id: source.item_id,
            name: source.name,
            relative_path: source.relative_path,
            is_directory: source.is_directory,
            files_total: source.files_total,
            folders_total: source.folders_total,
            bytes_total: source.bytes_total,
            status: source.status,
            error_message: source.error_message,
        })
        .collect();
    MigrationView {
        id: job.id,
        source_label: match job.source_kind.as_str() {
            "my-drive" => "My Drive".into(),
            "shared-drive" => "Shared Drive".into(),
            _ => "Shared with Me".into(),
        },
        operation_label: if job.destination_kind == "local"
            || job.operation_kind == "local-download"
        {
            "Local Download".into()
        } else {
            "Google Drive Migration".into()
        },
        destination_kind: job.destination_kind.clone(),
        destination_path: job.destination_path.clone(),
        can_open_local: job.destination_kind == "local"
            && !job.destination_path.is_empty()
            && FsPath::new(&job.destination_path).is_dir(),
        status: job.status,
        phase: job.phase,
        destination_url: job.destination_url,
        destination_label: if job.destination_kind == "local" {
            job.destination_path.clone()
        } else if job.destination_drive_name.is_empty() {
            "Not selected".into()
        } else if job.destination_folder_name == job.destination_drive_name {
            job.destination_drive_name
        } else {
            format!(
                "{} / {}",
                job.destination_drive_name, job.destination_folder_name
            )
        },
        files_total: job.files_total,
        folders_total: job.folders_total,
        size_label: format_bytes(job.bytes_total),
        files_copied: job.files_copied,
        copied_size_label: format_bytes(job.bytes_copied),
        exceptions_count: job.exceptions_count,
        created_at: job.created_at,
        started_at: job.started_at,
        completed_at: job.completed_at,
        copy_completed_at: job.copy_completed_at,
        resume_count: job.resume_count,
        error_message: job.error_message,
        archived_at: job.archived_at,
        can_cancel,
        can_archive,
        can_start,
        can_resume,
        running,
        allows_my_drive_destination,
        allows_google_destination,
        sources,
    }
}

fn google_drive_folder_id(url: &str) -> Result<String, String> {
    let url = url.trim();
    if !url.starts_with("https://drive.google.com/") {
        return Err("Enter a Google Drive folder or Shared Drive URL.".to_string());
    }
    let id = url
        .split("/folders/")
        .nth(1)
        .and_then(|value| value.split(['?', '/', '#']).next())
        .unwrap_or("");
    if id.len() < 10
        || !id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(
            "The Google Drive URL does not contain a valid destination folder ID.".to_string(),
        );
    }
    Ok(id.to_string())
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
    let (remotes, error, my_drive_ro_configured, my_drive_rw_configured) = match listed {
        Ok(configured_remotes) => {
            let my_drive_ro_configured = configured_remotes
                .iter()
                .any(|remote| remote.name == RemoteKind::MyDriveRo.name());
            let my_drive_rw_configured = configured_remotes
                .iter()
                .any(|remote| remote.name == RemoteKind::MyDriveRw.name());
            (
                configured_remotes
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
                my_drive_ro_configured,
                my_drive_rw_configured,
            )
        }
        Err(error) => {
            eprintln!("Unable to render remotes page: {error}");
            (Vec::new(), error.to_string(), false, false)
        }
    };
    let remote_setup_busy = matches!(google_remotes_state.ro, RemoteState::Configuring)
        || matches!(google_remotes_state.rw, RemoteState::Configuring);

    let template = RemotesTemplate {
        title: "Remotes - BOREAL",
        active_page: "remotes",
        alerts: build_alerts(
            &rclone_state,
            &google_client_state,
            bookmark_reminder_visible(&state),
        ),
        status_items: build_status_items(
            &state.download_state(),
            &rclone_state,
            &google_client_state,
            &google_remotes_state,
            &metadata_state,
            remotes.len(),
            authenticated_google_email(&state),
            &state.update_state(),
        ),
        poll_rclone: should_poll_ui(&rclone_state, &google_remotes_state, &metadata_state),
        remotes,
        error,
        my_drive_ro_configured,
        my_drive_rw_configured,
        remote_setup_busy,
        google_client_ready: matches!(google_client_state, GoogleClientState::Ready(_)),
        rclone_ready: matches!(rclone_state, RcloneState::Ready(_)),
    };
    render_template(&template)
}

async fn add_remote(
    State(state): State<Arc<AppState>>,
    Form(form): Form<AddRemoteForm>,
) -> Result<Redirect, StatusCode> {
    let kind = match form.remote_kind.as_str() {
        "my-drive-ro" => RemoteKind::MyDriveRo,
        "my-drive-rw" => RemoteKind::MyDriveRw,
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    AppState::configure_google_remote(state, kind).map_err(|error| {
        log::warn!("Unable to add {}: {error}", kind.label());
        StatusCode::CONFLICT
    })?;
    Ok(Redirect::to("/remotes"))
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
        "Shared with me",
        "/shared-with-me",
        "/shared-with-me/tags",
        "/shared-with-me/tags/remove",
        String::new(),
    )
}

async fn export_my_drive(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DrivePathQuery>,
) -> Result<Response<Body>, StatusCode> {
    export_drive_view(
        &state,
        &query,
        database::inventory::MY_DRIVE_SCOPE,
        "My Drive",
        "boreal-my-drive-report.xlsx",
    )
}

async fn export_shared_with_me(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DrivePathQuery>,
) -> Result<Response<Body>, StatusCode> {
    export_drive_view(
        &state,
        &query,
        database::inventory::SHARED_WITH_ME_SCOPE,
        "Shared with me",
        "boreal-shared-with-me-report.xlsx",
    )
}

fn export_drive_view(
    state: &AppState,
    query: &DrivePathQuery,
    inventory_scope: &str,
    view_name: &str,
    filename: &'static str,
) -> Result<Response<Body>, StatusCode> {
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let sort = match query.sort.as_str() {
        "type" | "size" | "modified" | "owner" => query.sort.as_str(),
        _ => "name",
    };
    let (exclude_owner, owner_filter) = match query.owner_filter.strip_prefix('!') {
        Some(owner) => (true, owner.trim()),
        None => (false, query.owner_filter.trim()),
    };
    let items = database::inventory::list_drive_directory(
        &database,
        inventory_scope,
        (!query.path.is_empty()).then_some(query.path.as_str()),
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
        query.direction == "desc",
    )
    .map_err(|error| {
        eprintln!("Unable to export {view_name}: {error}");
        StatusCode::BAD_REQUEST
    })?;
    let context = drive_export_context(view_name, query, sort, items.len());
    let rows = items
        .into_iter()
        .map(|item| {
            let size_bytes = item.size_bytes.unwrap_or(0);
            let drive_url = if item.is_directory {
                format!("https://drive.google.com/drive/folders/{}", item.item_id)
            } else {
                format!("https://drive.google.com/open?id={}", item.item_id)
            };
            vec![
                item.name.into(),
                if item.is_directory { "Folder" } else { "File" }.into(),
                item.relative_path.into(),
                item.mime_type.unwrap_or_default().into(),
                xlsx::Cell::Number(size_bytes),
                format_bytes(size_bytes).into(),
                item.modified_at.unwrap_or_default().into(),
                item.owner_email.unwrap_or_default().into(),
                item.permissions
                    .iter()
                    .map(|permission| permission.label.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
                    .into(),
                item.tags
                    .iter()
                    .map(|tag| tag.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
                    .into(),
                (if item.is_deleted { "Yes" } else { "No" }).into(),
                xlsx::Cell::Link {
                    url: drive_url,
                    label: "Open in Google Drive".to_string(),
                },
            ]
        })
        .collect::<Vec<_>>();
    let bytes = xlsx::workbook(
        &context,
        &[
            "Name",
            "Type",
            "Path",
            "MIME type",
            "Size (bytes)",
            "Size",
            "Modified",
            "Owner",
            "Permissions",
            "Tags",
            "Deleted",
            "Google Drive",
        ],
        &rows,
    )
    .map_err(|error| {
        eprintln!("Unable to build Excel report: {error}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    xlsx_download(bytes, filename)
}

fn drive_export_context(
    view_name: &str,
    query: &DrivePathQuery,
    sort: &str,
    result_count: usize,
) -> Vec<(String, String)> {
    vec![
        ("View".into(), view_name.into()),
        (
            "Location".into(),
            if query.path.is_empty() {
                view_name.into()
            } else {
                query.path.clone()
            },
        ),
        ("Results".into(), result_count.to_string()),
        ("Search".into(), filter_value(&query.q)),
        ("Tag".into(), filter_value(&query.tag)),
        ("Type".into(), filter_value(&query.type_filter)),
        ("Size".into(), filter_value(&query.size_filter)),
        ("Modified".into(), filter_value(&query.modified_filter)),
        ("Owner".into(), filter_value(&query.owner_filter)),
        ("Permission".into(), filter_value(&query.permission_filter)),
        (
            "Owner person tag".into(),
            filter_value(&query.owner_identity_tag),
        ),
        (
            "Permission person tag".into(),
            filter_value(&query.permission_identity_tag),
        ),
        ("Include deleted".into(), query.include_deleted.to_string()),
        (
            "Sort".into(),
            format!(
                "{sort} {}",
                if query.direction == "desc" {
                    "desc"
                } else {
                    "asc"
                }
            ),
        ),
    ]
}

fn filter_value(value: &str) -> String {
    if value.trim().is_empty() {
        "Any".to_string()
    } else {
        value.to_string()
    }
}

fn xlsx_download(bytes: Vec<u8>, filename: &'static str) -> Result<Response<Body>, StatusCode> {
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        )
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(bytes))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn export_shared_drives(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DrivePathQuery>,
) -> Result<Response<Body>, StatusCode> {
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if !query.drive.is_empty() {
        let drive = database::inventory::get_shared_drive(&database, &query.drive)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;
        return export_drive_view(
            &state,
            &query,
            &drive.inventory_scope,
            &format!("Shared Drive: {}", drive.name),
            "boreal-shared-drive-content-report.xlsx",
        );
    }

    let mut drives = database::inventory::list_shared_drives_filtered(
        &database,
        &query.q,
        &query.tag,
        &query.files_filter,
        &query.folders_filter,
        &query.size_filter,
        &query.modified_filter,
        &query.shared_drive_manager_filter,
        &query.shared_drive_permission_filter,
    )
    .map_err(|error| {
        eprintln!("Unable to export Shared Drives: {error}");
        StatusCode::BAD_REQUEST
    })?
    .into_iter()
    .filter(|drive| query.show_inaccessible || drive.is_accessible)
    .collect::<Vec<_>>();
    sort_shared_drives(&mut drives, &query);

    let context = vec![
        ("View".into(), "Shared Drives".into()),
        ("Results".into(), drives.len().to_string()),
        ("Search".into(), filter_value(&query.q)),
        ("Tag".into(), filter_value(&query.tag)),
        ("Files".into(), filter_value(&query.files_filter)),
        ("Folders".into(), filter_value(&query.folders_filter)),
        ("Total size".into(), filter_value(&query.size_filter)),
        ("Modified".into(), filter_value(&query.modified_filter)),
        (
            "Manager".into(),
            filter_value(&query.shared_drive_manager_filter),
        ),
        (
            "Permission".into(),
            filter_value(&query.shared_drive_permission_filter),
        ),
        (
            "Show inaccessible".into(),
            query.show_inaccessible.to_string(),
        ),
        (
            "Sort".into(),
            format!("{} {}", shared_drive_sort(&query), query.direction),
        ),
    ];
    let rows = drives
        .into_iter()
        .map(|drive| {
            let managers = drive
                .permission_identities
                .iter()
                .filter(|identity| {
                    identity.roles.iter().any(|role| {
                        role.eq_ignore_ascii_case("organizer") || role.eq_ignore_ascii_case("owner")
                    })
                })
                .map(|identity| identity.label.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            vec![
                drive.name.into(),
                drive.drive_id.clone().into(),
                (if drive.is_accessible { "Yes" } else { "No" }).into(),
                xlsx::Cell::Number(drive.files_scanned),
                xlsx::Cell::Number(drive.folders_scanned),
                xlsx::Cell::Number(drive.bytes_discovered),
                format_bytes(drive.bytes_discovered).into(),
                drive.modified_at.clone().into(),
                xlsx::Cell::Number(drive.permissions_scanned),
                managers.into(),
                drive
                    .permission_identities
                    .iter()
                    .map(|identity| identity.label.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
                    .into(),
                drive
                    .tags
                    .iter()
                    .map(|tag| tag.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
                    .into(),
                xlsx::Cell::Link {
                    url: format!("https://drive.google.com/drive/folders/{}", drive.drive_id),
                    label: "Open in Google Drive".into(),
                },
            ]
        })
        .collect::<Vec<_>>();
    let bytes = xlsx::workbook(
        &context,
        &[
            "Name",
            "Drive ID",
            "Accessible",
            "Files",
            "Folders",
            "Total size (bytes)",
            "Total size",
            "Modified",
            "Permission references",
            "Managers",
            "Permissions",
            "Tags",
            "Google Drive",
        ],
        &rows,
    )
    .map_err(|error| {
        eprintln!("Unable to build Shared Drives Excel report: {error}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    xlsx_download(bytes, "boreal-shared-drives-report.xlsx")
}

fn shared_drive_sort(query: &DrivePathQuery) -> &str {
    match query.sort.as_str() {
        "tags" | "files" | "folders" | "size" | "modified" | "managers" | "permissions" => {
            &query.sort
        }
        _ => "name",
    }
}

fn sort_shared_drives(drives: &mut [database::inventory::SharedDriveRow], query: &DrivePathQuery) {
    let sort = shared_drive_sort(query);
    drives.sort_by(|left, right| {
        let ordering = match sort {
            "tags" => left
                .tags
                .first()
                .map(|tag| tag.name.to_ascii_lowercase())
                .cmp(&right.tags.first().map(|tag| tag.name.to_ascii_lowercase())),
            "files" => left.files_scanned.cmp(&right.files_scanned),
            "folders" => left.folders_scanned.cmp(&right.folders_scanned),
            "size" => left.bytes_discovered.cmp(&right.bytes_discovered),
            "modified" => left.modified_at.cmp(&right.modified_at),
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
            &query.modified_filter,
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
        let sort = shared_drive_sort(&query).to_string();
        sort_shared_drives(&mut filtered_drives, &query);
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
                    modified_at: if drive.modified_at.is_empty() {
                        "—".to_string()
                    } else {
                        drive.modified_at
                    },
                    tags: drive
                        .tags
                        .into_iter()
                        .map(|tag| TagPill {
                            text_color: tag_text_color(&tag.color),
                            name: tag.name,
                            description: tag.description,
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
        let filter_tags = tags
            .iter()
            .map(|tag| TagFilterPill {
                slug: tag.slug.clone(),
                name: tag.name.clone(),
                description: tag.description.clone(),
                color: tag.color.clone(),
                text_color: tag_text_color(&tag.color),
                selected: query.tag == tag.slug,
            })
            .collect();
        let rclone_state = state.rclone_state();
        let google_client_state = state.google_client_state();
        let google_remotes_state = state.google_remotes_state();
        let metadata_state = state.metadata_state();
        return render_template(&SharedDrivesTemplate {
            title: "Shared Drives - BOREAL",
            active_page: "shared-drives",
            alerts: build_alerts(
                &rclone_state,
                &google_client_state,
                bookmark_reminder_visible(&state),
            ),
            status_items: build_status_items(
                &state.download_state(),
                &rclone_state,
                &google_client_state,
                &google_remotes_state,
                &metadata_state,
                configured_remote_count(&state.runtime, &rclone_state),
                authenticated_google_email(&state),
                &state.update_state(),
            ),
            poll_rclone: should_poll_ui(&rclone_state, &google_remotes_state, &metadata_state),
            drives,
            show_inaccessible: query.show_inaccessible,
            inaccessible_count,
            tags,
            filter_tags,
            search: query.q,
            tag_filter: query.tag,
            tagged_count: query.tagged,
            untagged_count: query.untagged,
            files_filter: query.files_filter,
            folders_filter: query.folders_filter,
            size_filter: query.size_filter,
            modified_filter: query.modified_filter,
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
    let display = identity_display(identity.label.clone(), identity.known, &identity.tags);
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
        label: display.label,
        roles_label,
        tagged: display.tagged,
        unknown: display.unknown,
        color: display.color,
        text_color: display.text_color,
        tag_details: display.tag_details,
        directory_url: display.directory_url,
    }
}

fn render_drive_explorer(
    state: &AppState,
    query: DrivePathQuery,
    inventory_scope: &str,
    active_page: &'static str,
    heading: &str,
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
    let include_deleted =
        query.include_deleted || query.tag == database::inventory::DELETED_TAG_FILTER;
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
        include_deleted,
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
                        include_deleted,
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
                        description: tag.description,
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
    let mut filter_tags = tags
        .iter()
        .map(|tag| TagFilterPill {
            slug: tag.slug.clone(),
            name: tag.name.clone(),
            description: tag.description.clone(),
            color: tag.color.clone(),
            text_color: tag_text_color(&tag.color),
            selected: query.tag == tag.slug,
        })
        .collect::<Vec<_>>();
    filter_tags.push(TagFilterPill {
        slug: database::inventory::DELETED_TAG_FILTER.to_string(),
        name: "Deleted".to_string(),
        description:
            "Items no longer present in Google Drive but retained in BOREAL's local inventory."
                .to_string(),
        color: "#6c757d".to_string(),
        text_color: "#ffffff",
        selected: query.tag == database::inventory::DELETED_TAG_FILTER,
    });
    let directory_tags = database::inventory::list_tags_for_scope(
        &database,
        database::inventory::TagScope::Directory,
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let identity_filter_tags = directory_tags
        .iter()
        .map(|tag| IdentityTagFilterPill {
            slug: tag.slug.clone(),
            name: tag.name.clone(),
            description: tag.description.clone(),
            color: tag.color.clone(),
            text_color: tag_text_color(&tag.color),
            owner_selected: query.owner_identity_tag == tag.slug,
            permission_selected: query.permission_identity_tag == tag.slug,
        })
        .collect();

    let template = MyDriveTemplate {
        title: heading.to_string(),
        active_page,
        alerts: build_alerts(
            &rclone_state,
            &google_client_state,
            bookmark_reminder_visible(state),
        ),
        status_items: build_status_items(
            &state.download_state(),
            &rclone_state,
            &google_client_state,
            &google_remotes_state,
            &metadata_state,
            configured_remote_count(&state.runtime, &rclone_state),
            authenticated_google_email(&state),
            &state.update_state(),
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
        filter_tags,
        identity_filter_tags,
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
        include_deleted,
        heading: heading.to_string(),
        root_label: root_label.to_string(),
        explorer_path: explorer_path.to_string(),
        export_path: match active_page {
            "my-drive" => "/my-drive/export.xlsx",
            "shared-with-me" => "/shared-with-me/export.xlsx",
            _ => "/shared-drives/export.xlsx",
        },
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
                    .map(|tag| {
                        if tag.description.is_empty() {
                            tag.name.clone()
                        } else {
                            format!("{}: {}", tag.name, tag.description)
                        }
                    })
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
        "/shared-drives?q={}&tag={}&show_inaccessible={}&files_filter={}&folders_filter={}&size_filter={}&modified_filter={}&shared_drive_manager_filter={}&shared_drive_permission_filter={}&sort={}&direction={}&{}={changed}",
        encode_query_value(&form.q),
        encode_query_value(&form.tag_filter),
        form.show_inaccessible,
        encode_query_value(&form.files_filter),
        encode_query_value(&form.folders_filter),
        encode_query_value(&form.size_filter),
        encode_query_value(&form.modified_filter),
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

async fn ui_download_status(
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, StatusCode> {
    render_download_status(&state.download_state())
}

async fn ui_download_status_item(
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, StatusCode> {
    render_template(&DownloadStatusItemTemplate {
        item: build_download_status_item(&state.download_state()),
    })
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
        alerts: build_alerts(
            &rclone_state,
            &google_client_state,
            bookmark_reminder_visible(&state),
        ),
        status_items: build_status_items(
            &state.download_state(),
            &rclone_state,
            &google_client_state,
            &google_remotes_state,
            &metadata_state,
            configured_remote_count(&state.runtime, &rclone_state),
            authenticated_google_email(&state),
            &state.update_state(),
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
        alerts: build_alerts(
            &rclone_state,
            &google_client_state,
            bookmark_reminder_visible(&state),
        ),
        status_items: build_status_items(
            &state.download_state(),
            &rclone_state,
            &google_client_state,
            &google_remotes_state,
            &metadata_state,
            configured_remote_count(&state.runtime, &rclone_state),
            authenticated_google_email(&state),
            &state.update_state(),
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
        alerts: build_alerts(
            &rclone_state,
            &google_client_state,
            bookmark_reminder_visible(&state),
        ),
        status_items: build_status_items(
            &state.download_state(),
            &rclone_state,
            &google_client_state,
            &google_remotes_state,
            &metadata_state,
            configured_remote_count(&state.runtime, &rclone_state),
            authenticated_google_email(state),
            &state.update_state(),
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
        alerts: build_alerts(
            &rclone_state,
            &google_client_state,
            bookmark_reminder_visible(&state),
        ),
        status_items: build_status_items(
            &state.download_state(),
            &rclone_state,
            &google_client_state,
            &google_remotes_state,
            &metadata_state,
            configured_remote_count(&state.runtime, &rclone_state),
            authenticated_google_email(&state),
            &state.update_state(),
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
    database::inventory::create_tag_with_description_and_scopes(
        &database,
        &form.name,
        &form.description,
        &form.color,
        &scopes,
    )
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
    database::inventory::update_tag_with_description_and_scopes(
        &database,
        &form.slug,
        &form.name,
        &form.description,
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

async fn delete_tag(
    State(state): State<Arc<AppState>>,
    Form(form): Form<DeleteTagForm>,
) -> Result<Redirect, StatusCode> {
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    database::inventory::delete_tag(&database, &form.slug).map_err(|error| {
        eprintln!("Unable to delete tag: {error}");
        StatusCode::BAD_REQUEST
    })?;
    println!("Tag deleted: slug={}", form.slug);
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
        alerts: build_alerts(
            &rclone_state,
            &google_client_state,
            bookmark_reminder_visible(&state),
        ),

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
            &state.download_state(),
            &rclone_state,
            &google_client_state,
            &google_remotes_state,
            &metadata_state,
            configured_remote_count(&state.runtime, &rclone_state),
            authenticated_google_email(&state),
            &state.update_state(),
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
        initial_setup_complete: setup_percent == 100 && metadata_setup_decided(&state),
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
        "Indexing My Drive metadata" => (true, false, 85, phase.to_string()),
        "Connecting" | "Downloading directory spreadsheet" | "Importing directory spreadsheet" => {
            (false, false, 0, "Waiting".to_string())
        }
        _ => (false, true, 100, "Complete".to_string()),
    };
    let shared_drives = if phase == "Discovering Shared Drives"
        || phase.starts_with("Fetching Shared Drive managers ")
        || phase.starts_with("Scanning Shared Drive ")
    {
        (true, false, shared_drive_percent, phase.to_string())
    } else {
        (false, false, 0, "Waiting".to_string())
    };
    let shared_with_me = match phase {
        "Fetching Shared with me metadata" => (true, false, 45, phase.to_string()),
        "Indexing Shared with me metadata" => (true, false, 85, phase.to_string()),
        "Connecting"
        | "Downloading directory spreadsheet"
        | "Importing directory spreadsheet"
        | "Fetching My Drive metadata"
        | "Indexing My Drive metadata" => (false, false, 0, "Waiting".to_string()),
        _ => (false, true, 100, "Complete".to_string()),
    };

    [
        ("Persons", selection.directory_info, "", directory),
        ("My Drive", selection.my_drive, "my-drive", my_drive),
        (
            "Shared with me",
            selection.shared_with_me,
            "shared-with-me",
            shared_with_me,
        ),
        (
            "Shared Drives",
            selection.shared_drives,
            "shared-drives",
            shared_drives,
        ),
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

fn metadata_setup_decided(state: &AppState) -> bool {
    latest_my_drive_summary(state).is_some()
        || latest_shared_summary(state).is_some()
        || latest_shared_drives_summary(state).is_some()
        || state
            .database()
            .ok()
            .and_then(|database| database::settings::metadata_setup_skipped(&database).ok())
            .unwrap_or(false)
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
            log::info!("Google Client ID credentials imported successfully");

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
    if let Ok(database) = state.database() {
        database::settings::set_metadata_setup_skipped(&database, false)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    AppState::start_metadata_update(state, selection).map_err(|error| {
        eprintln!("Unable to start metadata update: {error}");
        StatusCode::CONFLICT
    })?;

    Ok(Redirect::to("/"))
}

async fn start_selected_metadata_update(
    State(state): State<Arc<AppState>>,
    Form(form): Form<SelectedMetadataUpdateForm>,
) -> Result<Redirect, StatusCode> {
    let item_ids = comma_separated_values(&form.selected_item_ids);
    let drive_ids = comma_separated_values(&form.selected_drive_ids);
    let redirect = if !drive_ids.is_empty() {
        "/shared-drives".to_string()
    } else if form.inventory_scope == database::inventory::MY_DRIVE_SCOPE {
        "/my-drive".to_string()
    } else if form.inventory_scope == database::inventory::SHARED_WITH_ME_SCOPE {
        "/shared-with-me".to_string()
    } else if form
        .inventory_scope
        .starts_with(database::inventory::SHARED_DRIVE_SCOPE_PREFIX)
    {
        format!("/shared-drives?drive={}", encode_query_value(&form.drive))
    } else {
        return Err(StatusCode::BAD_REQUEST);
    };
    AppState::start_selected_metadata_update(state, form.inventory_scope, item_ids, drive_ids)
        .map_err(|error| {
            log::warn!("Unable to update selected metadata: {error}");
            StatusCode::CONFLICT
        })?;
    Ok(Redirect::to(&redirect))
}

fn comma_separated_values(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

async fn skip_setup_metadata(State(state): State<Arc<AppState>>) -> Result<Redirect, StatusCode> {
    let database = state
        .database()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    database::settings::set_metadata_setup_skipped(&database, true)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    log::info!("Optional initial metadata update skipped");
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
    show_bookmark_reminder: bool,
) -> Vec<AlertItem> {
    let mut alerts = Vec::new();

    if show_bookmark_reminder {
        alerts.push(AlertItem {
            level: "primary",
            icon: "bi-bookmark-star",
            message: "Bookmark this page so you can reopen BOREAL while it is running".to_string(),
            modal_target: "",
            dismiss_action: "/settings/bookmark-reminder/dismiss",
        });
    }

    match rclone_state {
        RcloneState::Initializing => {
            alerts.push(AlertItem {
                level: "warning",
                icon: "bi-hourglass-split",
                message: "BOREAL is initializing Rclone...".to_string(),
                modal_target: "",
                dismiss_action: "",
            });
        }

        RcloneState::Ready(_) => {}

        RcloneState::Error(error) => {
            alerts.push(AlertItem {
                level: "danger",
                icon: "bi-exclamation-triangle",
                message: format!("Rclone initialization failed: {error}"),
                modal_target: "",
                dismiss_action: "",
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
                dismiss_action: "",
            });
        }

        GoogleClientState::Ready(_) => {}

        GoogleClientState::Error(error) => {
            alerts.push(AlertItem {
                level: "danger",
                icon: "bi-key",
                message: format!("Google Client ID configuration is invalid: {error}"),
                modal_target: "googleClientSetupModal",
                dismiss_action: "",
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
    download_state: &DownloadState,
    rclone_state: &RcloneState,
    google_client_state: &GoogleClientState,
    _google_remotes_state: &GoogleRemotesState,
    metadata_state: &MetadataState,
    configured_remote_count: usize,
    google_account_email: String,
    update_state: &crate::update::UpdateState,
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
            title: "Open Rclone WebGUI".to_string(),
        },
        StatusItem {
            icon: "bi-google",
            label: "GDrive",
            value: google_account_value,
            value_class: google_account_class,
            value_url: String::new(),
            spinner: false,
            age_timestamp: String::new(),
            title: String::new(),
        },
        StatusItem {
            icon: "bi-key",
            label: "ClientID",
            value: client_id_value,
            value_class: client_id_value_class,
            value_url: String::new(),
            spinner: false,
            age_timestamp: String::new(),
            title: String::new(),
        },
        StatusItem {
            icon: "bi-cloud",
            label: "Remotes",
            value: remote_value,
            value_class: remote_class,
            value_url: String::new(),
            spinner: false,
            age_timestamp: String::new(),
            title: String::new(),
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
            title: String::new(),
        },
        build_download_status_item(download_state),
        StatusItem {
            icon: "bi-info-circle",
            label: "BOREAL",
            value: boreal_version_label(),
            value_class: if matches!(update_state, crate::update::UpdateState::Available { .. }) {
                "text-warning fw-semibold"
            } else {
                "text-success"
            },
            value_url: "/update".to_string(),
            spinner: false,
            age_timestamp: String::new(),
            title: match update_state {
                crate::update::UpdateState::Available { release } => {
                    format!("BOREAL v{} is available", release.version)
                }
                crate::update::UpdateState::Checking => "Checking for BOREAL updates".to_string(),
                crate::update::UpdateState::Current { .. } => {
                    "BOREAL is up to date. Open Update page.".to_string()
                }
                crate::update::UpdateState::Error(_) => {
                    "Open the Update page to retry the version check".to_string()
                }
            },
        },
    ]
}

fn boreal_version_label() -> String {
    format!("v{} ({})", env!("CARGO_PKG_VERSION"), boreal_maturity())
}

fn boreal_maturity() -> &'static str {
    static MATURITY: OnceLock<String> = OnceLock::new();

    let maturity = MATURITY.get_or_init(|| {
        let metadata: serde_json::Value = serde_json::from_str(include_str!("../../metadata.json"))
            .expect("embedded metadata.json should be valid JSON");
        metadata
            .pointer("/METADATA/maturity")
            .and_then(serde_json::Value::as_str)
            .expect("metadata.json should define METADATA.maturity")
            .to_string()
    });

    maturity
}

fn build_download_status_item(download_state: &DownloadState) -> StatusItem {
    let (value, value_class, spinner) = match download_state {
        DownloadState::Idle => ("Idle".to_string(), "text-body-secondary", false),
        DownloadState::Running { item_name, .. } => {
            (format!("Downloading {item_name}"), "text-primary", true)
        }
        DownloadState::Complete { item_name, .. } => {
            (format!("Complete: {item_name}"), "text-success", false)
        }
        DownloadState::Error { item_name, .. } => (
            if item_name.is_empty() {
                "Failed".to_string()
            } else {
                format!("Failed: {item_name}")
            },
            "text-danger",
            false,
        ),
    };
    StatusItem {
        icon: "bi-download",
        label: "Download",
        value,
        value_class,
        value_url: String::new(),
        spinner,
        age_timestamp: String::new(),
        title: String::new(),
    }
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
    fn persons_csv_template_has_supported_import_columns() {
        assert_eq!(
            PERSONS_CSV_TEMPLATE,
            "name,email,organization,type,status,departure_date,notes\r\n"
        );
    }

    #[test]
    fn formats_dashboard_sizes_with_one_decimal_and_adaptive_units() {
        assert_eq!(format_bytes(2_208_400_000_000), "2.2 TB");
        assert_eq!(format_bytes(2_208_400_000), "2.2 GB");
        assert_eq!(format_bytes(2_208_400), "2.2 MB");
        assert_eq!(format_bytes(2_208), "2.2 KB");
        assert_eq!(format_bytes(208), "208 B");
    }

    #[test]
    fn status_version_includes_release_maturity() {
        let metadata: serde_json::Value =
            serde_json::from_str(include_str!("../../metadata.json")).unwrap();
        let maturity = metadata["METADATA"]["maturity"].as_str().unwrap();

        assert_eq!(
            boreal_version_label(),
            format!("v{} ({maturity})", env!("CARGO_PKG_VERSION"))
        );
    }

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
    fn download_status_item_reflects_background_state() {
        let running = build_download_status_item(&DownloadState::Running {
            item_name: "Research".to_string(),
            destination: "/tmp/Research".to_string(),
        });
        assert!(running.spinner);
        assert_eq!(running.value, "Downloading Research");

        let complete = build_download_status_item(&DownloadState::Complete {
            item_name: "Research".to_string(),
            destination: "/tmp/Research".to_string(),
        });
        assert!(!complete.spinner);
        assert_eq!(complete.value, "Complete: Research");
        assert_eq!(complete.value_class, "text-success");
    }
}
