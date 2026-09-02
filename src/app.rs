use std::{
    collections::HashSet,
    sync::{Arc, Mutex, RwLock},
};

use std::process::Child;

use tokio::sync::watch;

use crate::{
    bootstrap::Runtime,
    database::{self, Database},
    google::{self, client::GoogleClientConfig},
    rclone::{
        self, RcloneStatus,
        remotes::{RemoteKind, RemoteState},
    },
};

pub struct AppState {
    pub runtime: Runtime,

    #[allow(dead_code)]
    pub database: DatabaseState,

    pub rclone: RwLock<RcloneState>,

    pub google_client: RwLock<GoogleClientState>,

    pub google_remotes: RwLock<GoogleRemotesState>,

    pub metadata: RwLock<MetadataState>,

    pub download: RwLock<DownloadState>,

    pub update: RwLock<crate::update::UpdateState>,

    metadata_job_active: Mutex<bool>,

    remote_setup_active: Mutex<bool>,

    rclone_gui: Mutex<Option<Child>>,

    shutdown_tx: watch::Sender<bool>,
}

#[derive(Debug, Clone)]
pub enum RcloneState {
    Initializing,

    Ready(RcloneStatus),

    Error(String),
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum GoogleClientState {
    NotConfigured,

    Ready(GoogleClientConfig),

    Error(String),
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum DatabaseState {
    Ready(Database),

    Error(String),
}

#[derive(Debug, Clone)]
pub struct GoogleRemotesState {
    pub rw: RemoteState,
    pub ro: RemoteState,
}

#[derive(Debug, Clone)]
pub enum MetadataState {
    NotSynchronized,
    Updating(MetadataProgress),
    Synchronized(MetadataSummary),
    Error(String),
}

#[derive(Debug, Clone)]
pub enum DownloadState {
    Idle,
    Running {
        item_name: String,
        destination: String,
    },
    Complete {
        item_name: String,
        destination: String,
    },
    Error {
        item_name: String,
        message: String,
    },
}

#[derive(Debug, Clone)]
pub struct MetadataProgress {
    pub selection: MetadataUpdateSelection,
    pub phase: String,
    pub files_scanned: u64,
    pub folders_scanned: u64,
    pub permissions_scanned: u64,
    pub bytes_discovered: u64,
    pub errors: u64,
}

#[derive(Debug, Clone)]
pub struct MetadataSummary {
    pub completed_at: String,
    pub files_scanned: u64,
    pub folders_scanned: u64,
    pub permissions_scanned: u64,
    pub bytes_discovered: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct MetadataUpdateSelection {
    pub my_drive: bool,
    pub shared_drives: bool,
    pub shared_with_me: bool,
    pub directory_info: bool,
}

impl AppState {
    pub fn new(runtime: Runtime) -> Self {
        let (shutdown_tx, _shutdown_rx) = watch::channel(false);

        let google_client = match google::client::detect(&runtime) {
            Ok(Some(config)) => {
                println!("Google Client ID configured.");

                GoogleClientState::Ready(config)
            }

            Ok(None) => {
                println!("Google Client ID is not configured.");

                GoogleClientState::NotConfigured
            }

            Err(error) => {
                let message = error.to_string();

                eprintln!("Google Client ID configuration error: {message}");

                GoogleClientState::Error(message)
            }
        };

        let database = match database::Database::initialize(&runtime) {
            Ok(database) => {
                println!("SQLite database ready: {}", database.path().display(),);

                DatabaseState::Ready(database)
            }

            Err(error) => {
                let message = error.to_string();

                eprintln!("SQLite database initialization failed: {message}");

                DatabaseState::Error(message)
            }
        };

        let metadata = match &database {
            DatabaseState::Ready(database) => match database::inventory::latest_summary(database) {
                Ok(Some(summary)) => MetadataState::Synchronized(MetadataSummary {
                    completed_at: summary.completed_at,
                    files_scanned: summary.files_scanned,
                    folders_scanned: summary.folders_scanned,
                    permissions_scanned: summary.permissions_scanned,
                    bytes_discovered: summary.bytes_discovered,
                }),
                Ok(None) => MetadataState::NotSynchronized,
                Err(error) => MetadataState::Error(error.to_string()),
            },
            DatabaseState::Error(_) => MetadataState::NotSynchronized,
        };

        Self {
            runtime,

            database,

            rclone: RwLock::new(RcloneState::Initializing),

            google_client: RwLock::new(google_client),

            google_remotes: RwLock::new(GoogleRemotesState {
                rw: RemoteState::Waiting,
                ro: RemoteState::Waiting,
            }),

            metadata: RwLock::new(metadata),

            download: RwLock::new(DownloadState::Idle),

            update: RwLock::new(crate::update::UpdateState::Checking),

            metadata_job_active: Mutex::new(false),

            remote_setup_active: Mutex::new(false),

            rclone_gui: Mutex::new(None),

            shutdown_tx,
        }
    }

    pub fn initialize_rclone(state: Arc<Self>) {
        tokio::spawn(async move {
            println!("Checking BOREAL-managed Rclone...");

            let worker_state = Arc::clone(&state);

            let result = tokio::task::spawn_blocking(move || {
                rclone::ensure_installed(&worker_state.runtime)
            })
            .await;

            let new_state = match result {
                Ok(Ok(mut status)) => {
                    println!("Rclone ready: {}", status.version);

                    println!("Rclone path: {}", status.path.display());

                    match state.rclone_gui.lock() {
                        Ok(mut gui) => {
                            if *state.shutdown_tx.borrow() {
                                RcloneState::Error(
                                    "Rclone WebGUI startup cancelled during shutdown".to_string(),
                                )
                            } else {
                                match rclone::gui::start(&state.runtime, &status.path) {
                                    Ok((child, gui_url)) => {
                                        status.gui_url = Some(gui_url);

                                        *gui = Some(child);

                                        state.refresh_google_remotes(&status.path);

                                        RcloneState::Ready(status)
                                    }

                                    Err(error) => RcloneState::Error(error.to_string()),
                                }
                            }
                        }

                        Err(error) => RcloneState::Error(format!(
                            "Unable to access Rclone WebGUI process: {error}"
                        )),
                    }
                }

                Ok(Err(error)) => {
                    let message = error.to_string();

                    eprintln!("Rclone initialization failed: {message}");

                    RcloneState::Error(message)
                }

                Err(error) => {
                    let message = format!("Rclone initialization task failed: {error}");

                    eprintln!("{message}");

                    RcloneState::Error(message)
                }
            };

            match state.rclone.write() {
                Ok(mut rclone) => {
                    *rclone = new_state;

                    println!("BOREAL initialization checks completed.");
                }

                Err(error) => {
                    eprintln!("Unable to update Rclone application state: {error}");
                }
            }
        });
    }

    pub fn check_for_updates(state: Arc<Self>) {
        log::info!(
            "BOREAL update check started: source={}",
            crate::update::CHANGELOG_URL
        );
        if let Ok(mut update) = state.update.write() {
            *update = crate::update::UpdateState::Checking;
        }
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(crate::update::check).await;
            let new_state = match result {
                Ok(state) => state,
                Err(error) => crate::update::UpdateState::Error(format!(
                    "BOREAL update check task failed: {error}"
                )),
            };
            if let Ok(mut update) = state.update.write() {
                *update = new_state;
            }
            match state.update_state() {
                crate::update::UpdateState::Available { release } => log::info!(
                    "BOREAL update available: current={}, latest={}, maturity={}",
                    env!("CARGO_PKG_VERSION"),
                    release.version,
                    release.maturity,
                ),
                crate::update::UpdateState::Current { latest } => log::info!(
                    "BOREAL update check completed: current={}, latest={}",
                    env!("CARGO_PKG_VERSION"),
                    latest.version,
                ),
                crate::update::UpdateState::Error(error) => {
                    log::warn!("BOREAL update check failed: {error}")
                }
                crate::update::UpdateState::Checking => {}
            }
        });
    }

    pub fn update_state(&self) -> crate::update::UpdateState {
        self.update
            .read()
            .map(|state| state.clone())
            .unwrap_or_else(|error| {
                crate::update::UpdateState::Error(format!("Unable to read update state: {error}"))
            })
    }

    pub fn rclone_state(&self) -> RcloneState {
        match self.rclone.read() {
            Ok(state) => state.clone(),

            Err(error) => {
                RcloneState::Error(format!("Unable to read Rclone application state: {error}"))
            }
        }
    }

    pub fn download_state(&self) -> DownloadState {
        self.download
            .read()
            .map(|state| state.clone())
            .unwrap_or_else(|error| DownloadState::Error {
                item_name: String::new(),
                message: format!("Unable to read download state: {error}"),
            })
    }

    pub fn set_download_state(&self, new_state: DownloadState) {
        if let Ok(mut state) = self.download.write() {
            *state = new_state;
        }
    }

    pub fn google_client_state(&self) -> GoogleClientState {
        match self.google_client.read() {
            Ok(state) => state.clone(),

            Err(error) => {
                GoogleClientState::Error(format!("Unable to read Google Client ID state: {error}"))
            }
        }
    }

    pub fn set_google_client_state(&self, new_state: GoogleClientState) {
        match self.google_client.write() {
            Ok(mut state) => {
                *state = new_state;
            }

            Err(error) => {
                eprintln!("Unable to update Google Client ID state: {error}");
            }
        }
    }

    pub fn google_remotes_state(&self) -> GoogleRemotesState {
        self.google_remotes
            .read()
            .map(|state| state.clone())
            .unwrap_or_else(|error| GoogleRemotesState {
                rw: RemoteState::Error(format!("Unable to read remote state: {error}")),
                ro: RemoteState::Error(format!("Unable to read remote state: {error}")),
            })
    }

    fn refresh_google_remotes(&self, executable: &std::path::Path) {
        let client = self.google_client_state();
        let Ok(mut remotes_state) = self.google_remotes.write() else {
            return;
        };

        match client {
            GoogleClientState::Ready(client) => {
                remotes_state.rw = rclone::remotes::detect(
                    &self.runtime,
                    executable,
                    &client,
                    RemoteKind::MyDriveRw,
                );
                remotes_state.ro = rclone::remotes::detect(
                    &self.runtime,
                    executable,
                    &client,
                    RemoteKind::MyDriveRo,
                );
            }
            _ => {
                remotes_state.rw = RemoteState::Waiting;
                remotes_state.ro = RemoteState::Waiting;
            }
        }
    }

    pub fn refresh_google_remotes_if_ready(&self) {
        if let RcloneState::Ready(status) = self.rclone_state() {
            self.refresh_google_remotes(&status.path);
        }
    }

    pub fn configure_google_remote(state: Arc<Self>, kind: RemoteKind) -> Result<(), String> {
        log::info!("Google remote setup requested: remote={}", kind.name());
        {
            let mut active = state
                .remote_setup_active
                .lock()
                .map_err(|error| format!("Unable to start remote setup: {error}"))?;
            if *active {
                return Err("Another remote setup is already running".to_string());
            }
            *active = true;
        }

        let executable = match state.rclone_state() {
            RcloneState::Ready(status) => status.path,
            _ => {
                state.finish_remote_setup();
                return Err("Rclone is not ready".to_string());
            }
        };
        let client = match state.google_client_state() {
            GoogleClientState::Ready(client) => client,
            _ => {
                state.finish_remote_setup();
                return Err("Google Client ID is not configured".to_string());
            }
        };

        if let Ok(mut remotes) = state.google_remotes.write() {
            *remote_state_mut(&mut remotes, kind) = RemoteState::Configuring;
        }

        tokio::spawn(async move {
            let worker_state = Arc::clone(&state);
            let result = tokio::task::spawn_blocking(move || {
                rclone::remotes::configure(&worker_state.runtime, &executable, &client, kind)
            })
            .await;

            let new_remote_state = match result {
                Ok(Ok(())) => RemoteState::Ready,
                Ok(Err(error)) => RemoteState::Error(error.to_string()),
                Err(error) => RemoteState::Error(format!("Remote setup task failed: {error}")),
            };

            if let Ok(mut remotes) = state.google_remotes.write() {
                *remote_state_mut(&mut remotes, kind) = new_remote_state;
            }
            match state.google_remotes_state() {
                remotes if matches!(remote_state(&remotes, kind), RemoteState::Ready) => {
                    log::info!("Google remote setup completed: remote={}", kind.name())
                }
                remotes => log::warn!(
                    "Google remote setup did not complete: remote={}, state={:?}",
                    kind.name(),
                    remote_state(&remotes, kind),
                ),
            }
            state.finish_remote_setup();
        });

        Ok(())
    }

    fn finish_remote_setup(&self) {
        if let Ok(mut active) = self.remote_setup_active.lock() {
            *active = false;
        }
    }

    pub fn start_migration_copy(state: Arc<Self>, migration_id: i64) -> Result<(), String> {
        let database = state.database()?;
        let job = database::migration::get(&database, migration_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("Unknown migration: {migration_id}"))?;
        if job.destination_drive_name.is_empty() || job.destination_folder_name.is_empty() {
            return Err("Validate a migration destination before starting the copy".to_string());
        }
        let executable = match state.rclone_state() {
            RcloneState::Ready(status) => status.path,
            _ => return Err("Rclone is not ready".to_string()),
        };
        let remotes = state.google_remotes_state();
        if !matches!(remotes.ro, RemoteState::Ready) {
            return Err("My Drive RO is not ready".to_string());
        }
        if !matches!(remotes.rw, RemoteState::Ready) {
            return Err("Add and authorize My Drive RW before starting a migration".to_string());
        }
        database::migration::begin_copy(&database, migration_id)
            .map_err(|error| error.to_string())?;
        log::info!(
            "Migration copy requested: migration_id={migration_id}, source_kind={}, sources={}, destination_drive_id={}, destination_folder_id={}",
            job.source_kind,
            job.sources.len(),
            job.destination_drive_id,
            job.destination_folder_id,
        );

        tokio::spawn(async move {
            let failure_database = database.clone();
            let result = tokio::task::spawn_blocking(move || -> Result<(), String> {
                if let Err(error) = rclone::migration::preflight_copy(
                    &state.runtime,
                    &executable,
                    &job.destination_drive_id,
                    &job.destination_folder_id,
                    &job.sources,
                ) {
                    let message = error.to_string();
                    let _ = database::migration::fail_preflight(&database, migration_id, &message);
                    return Err(message);
                }
                database::migration::confirm_copy_started(&database, migration_id)
                    .map_err(|error| error.to_string())?;
                log::info!("Migration preflight completed: migration_id={migration_id}");
                for source in &job.sources {
                    log::info!(
                        "Migration source copy started: migration_id={migration_id}, item_id={}, name={}",
                        source.item_id,
                        source.name,
                    );
                    database::migration::start_source(&database, migration_id, &source.item_id)
                        .map_err(|error| error.to_string())?;
                    if let Err(error) = rclone::migration::copy_source(
                        &state.runtime,
                        &executable,
                        &job.source_kind,
                        source,
                        &job.destination_drive_id,
                        &job.destination_folder_id,
                    ) {
                        let message = error.to_string();
                        let _ = database::migration::fail_copy(
                            &database,
                            migration_id,
                            Some(&source.item_id),
                            &message,
                        );
                        return Err(message);
                    }
                    database::migration::complete_source(&database, migration_id, &source.item_id)
                        .map_err(|error| error.to_string())?;
                    log::info!(
                        "Migration source copy completed: migration_id={migration_id}, item_id={}",
                        source.item_id,
                    );
                }
                database::migration::complete_copy(&database, migration_id)
                    .map_err(|error| error.to_string())?;
                log::info!("Migration copy completed: migration_id={migration_id}");
                Ok(())
            })
            .await;
            let task_error = match result {
                Ok(Ok(())) => None,
                Ok(Err(error)) => Some(error),
                Err(error) => Some(format!("Migration copy task failed: {error}")),
            };
            if let Some(message) = task_error {
                log::error!("Migration copy failed: migration_id={migration_id}, error={message}");
                let preflight_failed = database::migration::get(&failure_database, migration_id)
                    .ok()
                    .flatten()
                    .is_some_and(|job| job.status == "ready" && job.started_at.is_empty());
                if preflight_failed {
                    return;
                }
                let _ =
                    database::migration::fail_copy(&failure_database, migration_id, None, &message);
            }
        });
        Ok(())
    }

    pub fn database(&self) -> Result<Database, String> {
        match &self.database {
            DatabaseState::Ready(database) => Ok(database.clone()),

            DatabaseState::Error(error) => Err(error.clone()),
        }
    }

    pub fn metadata_state(&self) -> MetadataState {
        self.metadata
            .read()
            .map(|state| state.clone())
            .unwrap_or_else(|error| {
                MetadataState::Error(format!("Unable to read metadata state: {error}"))
            })
    }

    pub fn start_metadata_update(
        state: Arc<Self>,
        selection: MetadataUpdateSelection,
    ) -> Result<(), String> {
        if !selection.my_drive
            && !selection.shared_drives
            && !selection.shared_with_me
            && !selection.directory_info
        {
            return Err("Select at least one metadata source".to_string());
        }
        {
            let mut active = state
                .metadata_job_active
                .lock()
                .map_err(|error| format!("Unable to start metadata update: {error}"))?;

            if *active {
                return Err("A metadata update is already running".to_string());
            }

            *active = true;
        }

        let database = match state.database() {
            Ok(database) => database,
            Err(error) => {
                state.finish_metadata_job();
                return Err(error);
            }
        };

        let rclone_path = match state.rclone_state() {
            RcloneState::Ready(status) => status.path,
            _ => {
                state.finish_metadata_job();
                return Err("Rclone is not ready".to_string());
            }
        };
        let inventory_settings = match database::settings::load(&database) {
            Ok(settings) => settings,
            Err(error) => {
                state.finish_metadata_job();
                return Err(error.to_string());
            }
        };
        let permission_scanning = true;
        let directory_sheet_url = (selection.directory_info
            && inventory_settings.directory_sheet_enabled)
            .then(|| inventory_settings.directory_sheet_url.trim().to_string())
            .filter(|url| !url.is_empty());
        if selection.directory_info && directory_sheet_url.is_none() {
            state.finish_metadata_job();
            return Err(
                "Directory Info requires a configured directory spreadsheet URL".to_string(),
            );
        }
        let scan_id = match if selection.my_drive {
            database.start_scan_run("my-drive")
        } else {
            Ok(0)
        } {
            Ok(id) => id,
            Err(error) => {
                state.finish_metadata_job();
                return Err(error.to_string());
            }
        };
        let shared_scan_id = match if selection.shared_with_me {
            database.start_scan_run("shared-with-me")
        } else {
            Ok(0)
        } {
            Ok(id) => id,
            Err(error) => {
                let _ = database.fail_scan_run(scan_id, &error.to_string());
                state.finish_metadata_job();
                return Err(error.to_string());
            }
        };
        let shared_drives_scan_id = match if selection.shared_drives {
            database.start_scan_run("shared-drives")
        } else {
            Ok(0)
        } {
            Ok(id) => id,
            Err(error) => {
                let _ = database.fail_scan_run(scan_id, &error.to_string());
                let _ = database.fail_scan_run(shared_scan_id, &error.to_string());
                state.finish_metadata_job();
                return Err(error.to_string());
            }
        };

        println!(
            "Metadata update started: my_drive={}, shared_drives={}, shared_with_me={}, directory_info={}, remote=my-drive-ro, permissions={permission_scanning}",
            selection.my_drive,
            selection.shared_drives,
            selection.shared_with_me,
            selection.directory_info,
        );

        state.set_metadata_state(MetadataState::Updating(MetadataProgress {
            selection,
            phase: "Connecting".to_string(),
            files_scanned: 0,
            folders_scanned: 0,
            permissions_scanned: 0,
            bytes_discovered: 0,
            errors: 0,
        }));

        tokio::spawn(async move {
            let worker_state = Arc::clone(&state);
            let failure_database = database.clone();
            let result = tokio::task::spawn_blocking(move || {
                    match rclone::identity::fetch_read_only_account(
                        &worker_state.runtime,
                        &rclone_path,
                    ) {
                        Ok(identity) => {
                            database::directory::save_remote_account(
                                &database,
                                RemoteKind::MyDriveRo.name(),
                                identity.email.as_deref(),
                                identity.display_name.as_deref(),
                                identity.account_id.as_deref(),
                                &identity.raw_json,
                            )?;
                            println!(
                                "Authenticated Google account verified for remote={}",
                                RemoteKind::MyDriveRo.name(),
                            );
                        }
                        Err(error) => {
                            log::warn!("Unable to verify authenticated Google account: {error}");
                        }
                    }
                    if let Some(sheet_url) = directory_sheet_url.as_deref() {
                        worker_state.set_metadata_state(MetadataState::Updating(MetadataProgress {
                            selection,
                            phase: "Downloading directory spreadsheet".to_string(),
                            files_scanned: 0,
                            folders_scanned: 0,
                            permissions_scanned: 0,
                            bytes_discovered: 0,
                            errors: 0,
                        }));
                        match rclone::identity::download_google_sheet_csv(
                            &worker_state.runtime,
                            sheet_url,
                        ) {
                            Ok((location, csv)) => {
                                worker_state.set_metadata_state(MetadataState::Updating(MetadataProgress {
                                    selection,
                                    phase: "Importing directory spreadsheet".to_string(),
                                    files_scanned: 0,
                                    folders_scanned: 0,
                                    permissions_scanned: 0,
                                    bytes_discovered: 0,
                                    errors: 0,
                                }));
                                match database::directory::import_linked_sheet_csv(
                                    &database,
                                    sheet_url,
                                    &csv,
                                ) {
                                    Ok(summary) => log::info!(
                                        "Linked directory spreadsheet imported: spreadsheet_id={}, gid={}, rows={}, created={}, updated={}, rejected={}",
                                        location.spreadsheet_id,
                                        location.gid,
                                        summary.rows_seen,
                                        summary.rows_created,
                                        summary.rows_updated,
                                        summary.rows_rejected,
                                    ),
                                    Err(error) => {
                                        log::error!("Linked directory spreadsheet import failed: {error}");
                                        let _ = database::directory::record_linked_sheet_failure(
                                            &database,
                                            sheet_url,
                                            &error.to_string(),
                                        );
                                    }
                                }
                            }
                            Err(error) => {
                                log::error!("Linked directory spreadsheet download failed: {error}");
                                let _ = database::directory::record_linked_sheet_failure(
                                    &database,
                                    sheet_url,
                                    &error.to_string(),
                                );
                            }
                        }
                    }
                    let items = if selection.my_drive {
                        worker_state.set_metadata_state(MetadataState::Updating(MetadataProgress {
                            selection,
                            phase: "Fetching My Drive metadata".to_string(),
                            files_scanned: 0, folders_scanned: 0, permissions_scanned: 0,
                            bytes_discovered: 0, errors: 0,
                        }));
                        rclone::inventory::fetch_my_drive(
                            &worker_state.runtime, &rclone_path, scan_id, permission_scanning,
                        )?
                    } else {
                        Vec::new()
                    };
                    let mut unique_ids = HashSet::new();
                    let mut files = 0_u64;
                    let mut folders = 0_u64;
                    let mut bytes = 0_u64;
                    for item in &items {
                        if !unique_ids.insert(item.id.as_str()) {
                            continue;
                        }
                        if item.is_dir {
                            folders += 1;
                        } else {
                            files += 1;
                            if item.size >= 0 {
                                bytes = bytes.saturating_add(item.size as u64);
                            }
                        }
                    }
                    if selection.my_drive { println!(
                        "Metadata fetch completed: scan_id={scan_id}, listed_rows={}, unique_items={}, files={files}, folders={folders}, bytes={bytes}",
                        items.len(),
                        unique_ids.len(),
                    ); }
                    let my_drive_summary = if selection.my_drive {
                    worker_state.set_metadata_state(MetadataState::Updating(MetadataProgress {
                        selection,
                        phase: "Indexing My Drive metadata".to_string(),
                        files_scanned: files,
                        folders_scanned: folders,
                        permissions_scanned: 0,
                        bytes_discovered: bytes,
                        errors: 0,
                    }));
                    database::inventory::synchronize_my_drive(
                        &database,
                        scan_id,
                        &items,
                        permission_scanning,
                    )?
                    } else { database::inventory::latest_summary(&database)?.unwrap_or_default() };
                    let shared_items = if selection.shared_with_me {
                    worker_state.set_metadata_state(MetadataState::Updating(MetadataProgress {
                        selection,
                        phase: "Fetching Shared with me metadata".to_string(),
                        files_scanned: my_drive_summary.files_scanned,
                        folders_scanned: my_drive_summary.folders_scanned,
                        permissions_scanned: my_drive_summary.permissions_scanned,
                        bytes_discovered: my_drive_summary.bytes_discovered,
                        errors: 0,
                    }));
                    rclone::inventory::fetch_shared_with_me(
                        &worker_state.runtime,
                        &rclone_path,
                        shared_scan_id,
                        permission_scanning,
                    )?
                    } else { Vec::new() };
                    let shared_summary = if selection.shared_with_me {
                    worker_state.set_metadata_state(MetadataState::Updating(MetadataProgress {
                        selection,
                        phase: "Indexing Shared with me metadata".to_string(),
                        files_scanned: my_drive_summary.files_scanned,
                        folders_scanned: my_drive_summary.folders_scanned,
                        permissions_scanned: my_drive_summary.permissions_scanned,
                        bytes_discovered: my_drive_summary.bytes_discovered,
                        errors: 0,
                    }));
                    database::inventory::synchronize_drive(
                        &database,
                        database::inventory::SHARED_WITH_ME_SCOPE,
                        shared_scan_id,
                        &shared_items,
                        permission_scanning,
                    )?
                    } else {
                        database::inventory::latest_summary_for(&database, "shared-with-me")?.unwrap_or_default()
                    };
                    let mut shared_drives_summary = database::inventory::InventorySummary::default();
                    if selection.shared_drives {
                    worker_state.set_metadata_state(MetadataState::Updating(MetadataProgress {
                        selection,
                        phase: "Discovering Shared Drives".to_string(),
                        files_scanned: files,
                        folders_scanned: folders,
                        permissions_scanned: 0,
                        bytes_discovered: bytes,
                        errors: 0,
                    }));
                    let shared_drives = rclone::inventory::discover_shared_drives(
                        &worker_state.runtime,
                        &rclone_path,
                    )?;
                    let discovered = shared_drives.iter()
                        .map(|drive| (drive.id.clone(), drive.name.clone()))
                        .collect::<Vec<_>>();
                    database::inventory::reconcile_shared_drives(&database, &discovered)?;
                    log::info!("Shared Drive discovery completed: drives={}", shared_drives.len());
                    let mut shared_drive_errors = 0_u64;
                    if permission_scanning {
                        for (index, drive) in shared_drives.iter().enumerate() {
                            worker_state.set_metadata_state(MetadataState::Updating(MetadataProgress {
                                selection,
                                phase: format!(
                                    "Fetching Shared Drive managers {} of {}: {}",
                                    index + 1, shared_drives.len(), drive.name,
                                ),
                                files_scanned: 0,
                                folders_scanned: 0,
                                permissions_scanned: 0,
                                bytes_discovered: 0,
                                errors: shared_drive_errors,
                            }));
                            match rclone::identity::fetch_shared_drive_permissions(
                                &worker_state.runtime,
                                &drive.id,
                            ) {
                                Ok(permissions) => {
                                    database::inventory::record_shared_drive_permissions(
                                        &database,
                                        &drive.id,
                                        &permissions,
                                    )?;
                                    log::info!(
                                        "Shared Drive membership fetched: drive_id={}, permissions={}",
                                        drive.id,
                                        permissions.len(),
                                    );
                                }
                                Err(error) => {
                                    shared_drive_errors += 1;
                                    log::warn!(
                                        "Unable to refresh Shared Drive root permissions: drive_id={}, name={}, error={error}",
                                        drive.id,
                                        drive.name,
                                    );
                                }
                            }
                        }
                    }
                    for (index, drive) in shared_drives.iter().enumerate() {
                        worker_state.set_metadata_state(MetadataState::Updating(MetadataProgress {
                            selection,
                            phase: format!(
                                "Scanning Shared Drive {} of {}: {}",
                                index + 1, shared_drives.len(), drive.name,
                            ),
                            files_scanned: shared_drives_summary.files_scanned,
                            folders_scanned: shared_drives_summary.folders_scanned,
                            permissions_scanned: shared_drives_summary.permissions_scanned,
                            bytes_discovered: shared_drives_summary.bytes_discovered,
                            errors: shared_drive_errors,
                        }));
                        log::info!(
                            "Shared Drive scan started: drive_index={}, drive_count={}, drive_id={}, name={}",
                            index + 1, shared_drives.len(), drive.id, drive.name,
                        );
                        let drive_scan_id = database.start_scan_run(&format!("shared-drive:{}", drive.id))?;
                        let drive_result = rclone::inventory::fetch_shared_drive(
                            &worker_state.runtime, &rclone_path, drive_scan_id, &drive.id,
                            permission_scanning,
                        ).map_err(|error| error.to_string()).and_then(|drive_items| {
                            let summary = database::inventory::synchronize_drive(
                                &database,
                                &database::inventory::shared_drive_scope(&drive.id),
                                drive_scan_id,
                                &drive_items,
                                permission_scanning,
                            ).map_err(|error| error.to_string())?;
                            Ok(summary)
                        });
                        match drive_result {
                            Ok(drive_summary) => {
                                database::inventory::record_shared_drive_scan(
                                    &database, &drive.id, &drive_summary,
                                )?;
                                shared_drives_summary.files_scanned += drive_summary.files_scanned;
                                shared_drives_summary.folders_scanned += drive_summary.folders_scanned;
                                shared_drives_summary.permissions_scanned += drive_summary.permissions_scanned;
                                shared_drives_summary.bytes_discovered = shared_drives_summary.bytes_discovered
                                    .saturating_add(drive_summary.bytes_discovered);
                                shared_drives_summary.deleted_items += drive_summary.deleted_items;
                                log::info!(
                                    "Shared Drive scan completed: drive_id={}, name={}, files={}, folders={}, bytes={}, permissions={}",
                                    drive.id, drive.name, drive_summary.files_scanned,
                                    drive_summary.folders_scanned, drive_summary.bytes_discovered,
                                    drive_summary.permissions_scanned,
                                );
                            }
                            Err(error) => {
                                shared_drive_errors += 1;
                                let _ = database.fail_scan_run(drive_scan_id, &error);
                                let _ = database::inventory::record_shared_drive_error(&database, &drive.id, &error);
                                log::error!("Shared Drive scan failed: drive_id={}, name={}, error={error}", drive.id, drive.name);
                            }
                        }
                    }
                    shared_drives_summary = database::inventory::shared_drives_aggregate(&database)?;
                    database.complete_scan_run(shared_drives_scan_id, &shared_drives_summary)?;
                    }
                    Ok::<_, crate::database::DatabaseError>((my_drive_summary, shared_drives_summary, shared_summary))
                }).await;

            match result {
                Ok(Ok((summary, shared_drives_summary, shared_summary))) => {
                    if selection.shared_drives {
                        println!(
                            "Shared Drives update completed: scan_id={shared_drives_scan_id}, files={}, folders={}, permissions={}, bytes={}, deleted_items={}",
                            shared_drives_summary.files_scanned,
                            shared_drives_summary.folders_scanned,
                            shared_drives_summary.permissions_scanned,
                            shared_drives_summary.bytes_discovered,
                            shared_drives_summary.deleted_items,
                        );
                    }
                    if selection.my_drive {
                        println!(
                            "Metadata update completed: scan_id={scan_id}, files={}, folders={}, permissions={}, bytes={}, deleted_items={}",
                            summary.files_scanned,
                            summary.folders_scanned,
                            summary.permissions_scanned,
                            summary.bytes_discovered,
                            summary.deleted_items,
                        );
                    }
                    if selection.shared_with_me {
                        println!(
                            "Shared with me update completed: scan_id={shared_scan_id}, files={}, folders={}, permissions={}, bytes={}, deleted_items={}",
                            shared_summary.files_scanned,
                            shared_summary.folders_scanned,
                            shared_summary.permissions_scanned,
                            shared_summary.bytes_discovered,
                            shared_summary.deleted_items,
                        );
                    }
                    if summary.completed_at.is_empty() {
                        state.set_metadata_state(MetadataState::NotSynchronized);
                    } else {
                        state.set_metadata_state(MetadataState::Synchronized(MetadataSummary {
                            completed_at: summary.completed_at,
                            files_scanned: summary.files_scanned,
                            folders_scanned: summary.folders_scanned,
                            permissions_scanned: summary.permissions_scanned,
                            bytes_discovered: summary.bytes_discovered,
                        }));
                    }
                }
                Ok(Err(error)) => {
                    let message = error.to_string();
                    eprintln!("Metadata update failed: scan_id={scan_id}, error={message}");
                    if let Err(database_error) = failure_database.fail_scan_run(scan_id, &message) {
                        eprintln!(
                            "Unable to record metadata failure: scan_id={scan_id}, error={database_error}"
                        );
                    }
                    let _ = failure_database.fail_scan_run(shared_scan_id, &message);
                    let _ = failure_database.fail_scan_run(shared_drives_scan_id, &message);

                    state.set_metadata_state(MetadataState::Error(message));
                }
                Err(error) => {
                    let message = format!("Metadata worker failed: {error}");
                    eprintln!("Metadata update failed: scan_id={scan_id}, error={message}");
                    if let Err(database_error) = failure_database.fail_scan_run(scan_id, &message) {
                        eprintln!(
                            "Unable to record metadata failure: scan_id={scan_id}, error={database_error}"
                        );
                    }
                    let _ = failure_database.fail_scan_run(shared_scan_id, &message);
                    let _ = failure_database.fail_scan_run(shared_drives_scan_id, &message);
                    state.set_metadata_state(MetadataState::Error(message));
                }
            }

            state.finish_metadata_job();
        });

        Ok(())
    }

    pub fn start_selected_metadata_update(
        state: Arc<Self>,
        inventory_scope: String,
        item_ids: Vec<String>,
        drive_ids: Vec<String>,
    ) -> Result<(), String> {
        if item_ids.is_empty() && drive_ids.is_empty() {
            return Err("Select at least one Drive item or Shared Drive".to_string());
        }
        log::info!(
            "Selected metadata update requested: scope={inventory_scope}, items={}, shared_drives={}",
            item_ids.len(),
            drive_ids.len(),
        );
        {
            let mut active = state
                .metadata_job_active
                .lock()
                .map_err(|error| format!("Unable to start metadata update: {error}"))?;
            if *active {
                return Err("A metadata update is already running".to_string());
            }
            *active = true;
        }
        let database = state.database().map_err(|error| {
            state.finish_metadata_job();
            error
        })?;
        let executable = match state.rclone_state() {
            RcloneState::Ready(status) => status.path,
            _ => {
                state.finish_metadata_job();
                return Err("Rclone is not ready".to_string());
            }
        };
        if !matches!(state.google_remotes_state().ro, RemoteState::Ready) {
            state.finish_metadata_job();
            return Err("My Drive RO is not ready".to_string());
        }
        let previous_state = state.metadata_state();
        let selection = MetadataUpdateSelection {
            my_drive: inventory_scope == database::inventory::MY_DRIVE_SCOPE,
            shared_drives: !drive_ids.is_empty()
                || inventory_scope.starts_with(database::inventory::SHARED_DRIVE_SCOPE_PREFIX),
            shared_with_me: inventory_scope == database::inventory::SHARED_WITH_ME_SCOPE,
            directory_info: false,
        };
        state.set_metadata_state(MetadataState::Updating(MetadataProgress {
            selection,
            phase: "Refreshing selected metadata".to_string(),
            files_scanned: 0,
            folders_scanned: 0,
            permissions_scanned: 0,
            bytes_discovered: 0,
            errors: 0,
        }));
        tokio::spawn(async move {
            let worker_state = Arc::clone(&state);
            let result = tokio::task::spawn_blocking(move || -> Result<(), String> {
                for drive_id in drive_ids {
                    let scope = database::inventory::shared_drive_scope(&drive_id);
                    let scan_id = database
                        .start_scan_run(&format!("partial:{scope}"))
                        .map_err(|error| error.to_string())?;
                    let items = rclone::inventory::fetch_shared_drive(
                        &worker_state.runtime,
                        &executable,
                        scan_id,
                        &drive_id,
                        true,
                    )
                    .map_err(|error| error.to_string())?;
                    let summary = database::inventory::synchronize_drive(
                        &database, &scope, scan_id, &items, true,
                    )
                    .map_err(|error| error.to_string())?;
                    database::inventory::record_shared_drive_scan(&database, &drive_id, &summary)
                        .map_err(|error| error.to_string())?;
                }
                for item_id in item_ids {
                    let item = database::inventory::get_drive_download_item(
                        &database,
                        &inventory_scope,
                        &item_id,
                    )
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| format!("Unknown selected Drive item: {item_id}"))?;
                    let scan_id = database
                        .start_scan_run(&format!("partial:{inventory_scope}"))
                        .map_err(|error| error.to_string())?;
                    let items = rclone::inventory::fetch_selected_path(
                        &worker_state.runtime,
                        &executable,
                        &inventory_scope,
                        &item.relative_path,
                        item.is_directory,
                    )
                    .map_err(|error| error.to_string())?;
                    match items {
                        Some(items) => database::inventory::refresh_drive_items(
                            &database,
                            &inventory_scope,
                            scan_id,
                            &items,
                        ),
                        None => database::inventory::mark_drive_item_missing(
                            &database,
                            &inventory_scope,
                            scan_id,
                            &item_id,
                        ),
                    }
                    .map_err(|error| error.to_string())?;
                }
                Ok(())
            })
            .await;
            match result {
                Ok(Ok(())) => {
                    log::info!("Selected metadata update completed");
                    state.set_metadata_state(previous_state);
                }
                Ok(Err(message)) => {
                    log::error!("Selected metadata update failed: {message}");
                    state.set_metadata_state(MetadataState::Error(message));
                }
                Err(error) => {
                    log::error!("Selected metadata update task failed: {error}");
                    state.set_metadata_state(MetadataState::Error(format!(
                        "Selected metadata update task failed: {error}"
                    )));
                }
            }
            state.finish_metadata_job();
        });
        Ok(())
    }

    fn set_metadata_state(&self, new_state: MetadataState) {
        if let Ok(mut state) = self.metadata.write() {
            *state = new_state;
        }
    }

    fn finish_metadata_job(&self) {
        if let Ok(mut active) = self.metadata_job_active.lock() {
            *active = false;
        }
    }

    pub fn request_shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
    }

    pub fn shutdown_receiver(&self) -> watch::Receiver<bool> {
        self.shutdown_tx.subscribe()
    }

    pub fn stop_rclone_gui(&self) {
        let child = match self.rclone_gui.lock() {
            Ok(mut gui) => gui.take(),

            Err(error) => {
                eprintln!("Unable to access Rclone WebGUI process during shutdown: {error}");

                return;
            }
        };

        if let Some(mut child) = child {
            if let Err(error) = rclone::gui::stop(&mut child) {
                eprintln!("Unable to stop Rclone WebGUI: {error}");
            }
        }
    }
}

fn remote_state_mut(remotes: &mut GoogleRemotesState, kind: RemoteKind) -> &mut RemoteState {
    match kind {
        RemoteKind::MyDriveRw => &mut remotes.rw,
        RemoteKind::MyDriveRo => &mut remotes.ro,
    }
}

fn remote_state(remotes: &GoogleRemotesState, kind: RemoteKind) -> &RemoteState {
    match kind {
        RemoteKind::MyDriveRw => &remotes.rw,
        RemoteKind::MyDriveRo => &remotes.ro,
    }
}
