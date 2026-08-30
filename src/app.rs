use std::{
    collections::HashSet,
    sync::{
        Arc,
        Mutex,
        RwLock,
    },
};

use std::process::Child;

use tokio::sync::watch;

use crate::{
    bootstrap::Runtime,
    database::{
        self,
        Database,
    },
    google::{
        self,
        client::GoogleClientConfig,
    },
    rclone::{
        self,
        remotes::{
            RemoteKind,
            RemoteState,
        },
        RcloneStatus,
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

    metadata_job_active: Mutex<bool>,

    remote_setup_active: Mutex<bool>,

    rclone_gui: Mutex<Option<Child>>,

    shutdown_tx: watch::Sender<bool>,
}

#[derive(Debug, Clone)]
pub enum RcloneState {
    Initializing,

    Ready(
        RcloneStatus,
    ),

    Error(
        String,
    ),
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum GoogleClientState {
    NotConfigured,

    Ready(
        GoogleClientConfig,
    ),

    Error(
        String,
    ),
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum DatabaseState {
    Ready(
        Database,
    ),

    Error(
        String,
    ),
}

#[derive(Debug, Clone)]
pub struct GoogleRemotesState {
    pub rw: RemoteState,
    pub ro: RemoteState,
}

#[derive(Debug, Clone)]
pub enum MetadataState {
    NotSynchronized,
    Updating(
        MetadataProgress,
    ),
    Synchronized(
        MetadataSummary,
    ),
    Error(
        String,
    ),
}

#[derive(Debug, Clone)]
pub struct MetadataProgress {
    pub phase: &'static str,
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

impl AppState {
    pub fn new(
        runtime: Runtime,
    ) -> Self {
        let (
            shutdown_tx,
            _shutdown_rx,
        ) = watch::channel(
            false,
        );

        let google_client = match google::client::detect(
            &runtime,
        ) {
            Ok(
                Some(
                    config,
                ),
            ) => {
                println!(
                    "Google Client ID configured."
                );

                GoogleClientState::Ready(
                    config,
                )
            }

            Ok(
                None,
            ) => {
                println!(
                    "Google Client ID is not configured."
                );

                GoogleClientState::NotConfigured
            }

            Err(
                error,
            ) => {
                let message = error.to_string();

                eprintln!(
                    "Google Client ID configuration error: {message}"
                );

                GoogleClientState::Error(
                    message,
                )
            }
        };

        let database = match database::Database::initialize(
            &runtime,
        ) {
            Ok(
                database,
            ) => {
                println!(
                    "SQLite database ready: {}",
                    database.path().display(),
                );

                DatabaseState::Ready(
                    database,
                )
            }

            Err(
                error,
            ) => {
                let message = error.to_string();

                eprintln!(
                    "SQLite database initialization failed: {message}"
                );

                DatabaseState::Error(
                    message,
                )
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

            rclone: RwLock::new(
                RcloneState::Initializing,
            ),

            google_client: RwLock::new(
                google_client,
            ),

            google_remotes: RwLock::new(
                GoogleRemotesState {
                    rw: RemoteState::Waiting,
                    ro: RemoteState::Waiting,
                },
            ),

            metadata: RwLock::new(metadata),

            metadata_job_active: Mutex::new(
                false,
            ),

            remote_setup_active: Mutex::new(
                false,
            ),

            rclone_gui: Mutex::new(
                None,
            ),

            shutdown_tx,
        }
    }

    pub fn initialize_rclone(
        state: Arc<Self>,
    ) {
        tokio::spawn(
            async move {
                println!(
                    "Checking BOREAL-managed Rclone..."
                );

                let worker_state = Arc::clone(
                    &state,
                );

                let result = tokio::task::spawn_blocking(
                    move || {
                        rclone::ensure_installed(
                            &worker_state.runtime,
                        )
                    },
                )
                .await;

                let new_state = match result {
                    Ok(
                        Ok(
                            mut status,
                        ),
                    ) => {
                        println!(
                            "Rclone ready: {}",
                            status.version
                        );

                        println!(
                            "Rclone path: {}",
                            status.path.display()
                        );

                        match state.rclone_gui.lock() {
                            Ok(
                                mut gui,
                            ) => {
                                if *state.shutdown_tx.borrow() {
                                    RcloneState::Error(
                                        "Rclone WebGUI startup cancelled during shutdown"
                                            .to_string(),
                                    )
                                } else {
                                    match rclone::gui::start(
                                        &state.runtime,
                                        &status.path,
                                    ) {
                                        Ok(
                                            (
                                                child,
                                                gui_url,
                                            ),
                                        ) => {
                                            status.gui_url = Some(
                                                gui_url,
                                            );

                                            *gui = Some(
                                                child,
                                            );

                                            state.refresh_google_remotes(
                                                &status.path,
                                            );

                                            RcloneState::Ready(
                                                status,
                                            )
                                        }

                                        Err(
                                            error,
                                        ) => {
                                            RcloneState::Error(
                                                error.to_string(),
                                            )
                                        }
                                    }
                                }
                            }

                            Err(
                                error,
                            ) => {
                                RcloneState::Error(
                                    format!(
                                        "Unable to access Rclone WebGUI process: {error}"
                                    ),
                                )
                            }
                        }
                    }

                    Ok(
                        Err(
                            error,
                        ),
                    ) => {
                        let message = error.to_string();

                        eprintln!(
                            "Rclone initialization failed: {message}"
                        );

                        RcloneState::Error(
                            message,
                        )
                    }

                    Err(
                        error,
                    ) => {
                        let message = format!(
                            "Rclone initialization task failed: {error}"
                        );

                        eprintln!(
                            "{message}"
                        );

                        RcloneState::Error(
                            message,
                        )
                    }
                };

                match state.rclone.write() {
                    Ok(
                        mut rclone,
                    ) => {
                        *rclone = new_state;
                    }

                    Err(
                        error,
                    ) => {
                        eprintln!(
                            "Unable to update Rclone application state: {error}"
                        );
                    }
                }
            },
        );
    }

    pub fn rclone_state(
        &self,
    ) -> RcloneState {
        match self.rclone.read() {
            Ok(
                state,
            ) => state.clone(),

            Err(
                error,
            ) => {
                RcloneState::Error(
                    format!(
                        "Unable to read Rclone application state: {error}"
                    ),
                )
            }
        }
    }

    pub fn google_client_state(
        &self,
    ) -> GoogleClientState {
        match self.google_client.read() {
            Ok(
                state,
            ) => state.clone(),

            Err(
                error,
            ) => {
                GoogleClientState::Error(
                    format!(
                        "Unable to read Google Client ID state: {error}"
                    ),
                )
            }
        }
    }

    pub fn set_google_client_state(
        &self,
        new_state: GoogleClientState,
    ) {
        match self.google_client.write() {
            Ok(
                mut state,
            ) => {
                *state = new_state;
            }

            Err(
                error,
            ) => {
                eprintln!(
                    "Unable to update Google Client ID state: {error}"
                );
            }
        }
    }

    pub fn google_remotes_state(
        &self,
    ) -> GoogleRemotesState {
        self.google_remotes
            .read()
            .map(|state| state.clone())
            .unwrap_or_else(|error| GoogleRemotesState {
                rw: RemoteState::Error(format!(
                    "Unable to read remote state: {error}"
                )),
                ro: RemoteState::Error(format!(
                    "Unable to read remote state: {error}"
                )),
            })
    }

    fn refresh_google_remotes(
        &self,
        executable: &std::path::Path,
    ) {
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

    pub fn refresh_google_remotes_if_ready(
        &self,
    ) {
        if let RcloneState::Ready(status) = self.rclone_state() {
            self.refresh_google_remotes(&status.path);
        }
    }

    pub fn configure_google_remote(
        state: Arc<Self>,
        kind: RemoteKind,
    ) -> Result<(), String> {
        {
            let mut active = state.remote_setup_active
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
                rclone::remotes::configure(
                    &worker_state.runtime,
                    &executable,
                    &client,
                    kind,
                )
            }).await;

            let new_remote_state = match result {
                Ok(Ok(())) => RemoteState::Ready,
                Ok(Err(error)) => RemoteState::Error(error.to_string()),
                Err(error) => RemoteState::Error(format!(
                    "Remote setup task failed: {error}"
                )),
            };

            if let Ok(mut remotes) = state.google_remotes.write() {
                *remote_state_mut(&mut remotes, kind) = new_remote_state;
            }
            state.finish_remote_setup();
        });

        Ok(())
    }

    fn finish_remote_setup(
        &self,
    ) {
        if let Ok(mut active) = self.remote_setup_active.lock() {
            *active = false;
        }
    }

    pub fn database(
        &self,
    ) -> Result<Database, String> {
        match &self.database {
            DatabaseState::Ready(
                database,
            ) => Ok(
                database.clone(),
            ),

            DatabaseState::Error(
                error,
            ) => Err(
                error.clone(),
            ),
        }
    }

    pub fn metadata_state(
        &self,
    ) -> MetadataState {
        self.metadata
            .read()
            .map(
                |state| state.clone(),
            )
            .unwrap_or_else(
                |error| MetadataState::Error(
                    format!(
                        "Unable to read metadata state: {error}"
                    ),
                ),
            )
    }

    pub fn start_metadata_update(
        state: Arc<Self>,
    ) -> Result<(), String> {
        {
            let mut active = state.metadata_job_active
                .lock()
                .map_err(
                    |error| format!(
                        "Unable to start metadata update: {error}"
                    ),
                )?;

            if *active {
                return Err(
                    "A metadata update is already running"
                        .to_string(),
                );
            }

            *active = true;
        }

        let database = match state.database() {
            Ok(
                database,
            ) => database,
            Err(
                error,
            ) => {
                state.finish_metadata_job();
                return Err(
                    error,
                );
            }
        };

        let rclone_path = match state.rclone_state() {
            RcloneState::Ready(status) => status.path,
            _ => {
                state.finish_metadata_job();
                return Err("Rclone is not ready".to_string());
            }
        };
        let permission_scanning = match database::settings::load(&database) {
            Ok(settings) => settings.permission_scanning,
            Err(error) => {
                state.finish_metadata_job();
                return Err(error.to_string());
            }
        };
        let scan_id = match database.start_scan_run(
            "my-drive",
        ) {
            Ok(
                id,
            ) => id,
            Err(
                error,
            ) => {
                state.finish_metadata_job();
                return Err(
                    error.to_string(),
                );
            }
        };

        println!(
            "Metadata update started: scan_id={scan_id}, remote=my-drive-ro, permissions={permission_scanning}"
        );

        state.set_metadata_state(
            MetadataState::Updating(
                MetadataProgress {
                    phase: "Connecting",
                    files_scanned: 0,
                    folders_scanned: 0,
                    permissions_scanned: 0,
                    bytes_discovered: 0,
                    errors: 0,
                },
            ),
        );

        tokio::spawn(
            async move {
                let worker_state = Arc::clone(&state);
                let failure_database = database.clone();
                let result = tokio::task::spawn_blocking(move || {
                    let items = rclone::inventory::fetch_my_drive(
                        &worker_state.runtime,
                        &rclone_path,
                        scan_id,
                        permission_scanning,
                    )?;
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
                    println!(
                        "Metadata fetch completed: scan_id={scan_id}, listed_rows={}, unique_items={}, files={files}, folders={folders}, bytes={bytes}",
                        items.len(),
                        unique_ids.len(),
                    );
                    worker_state.set_metadata_state(MetadataState::Updating(MetadataProgress {
                        phase: "Saving My Drive metadata",
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
                    )
                }).await;

                match result {
                    Ok(Ok(summary)) => {
                        println!(
                            "Metadata update completed: scan_id={scan_id}, files={}, folders={}, permissions={}, bytes={}, deleted_items={}",
                            summary.files_scanned,
                            summary.folders_scanned,
                            summary.permissions_scanned,
                            summary.bytes_discovered,
                            summary.deleted_items,
                        );
                        state.set_metadata_state(
                            MetadataState::Synchronized(
                                MetadataSummary {
                                    completed_at: summary.completed_at,
                                    files_scanned: summary.files_scanned,
                                    folders_scanned: summary.folders_scanned,
                                    permissions_scanned: summary.permissions_scanned,
                                    bytes_discovered: summary.bytes_discovered,
                                },
                            ),
                        );
                    }
                    Ok(Err(error)) => {
                        let message = error.to_string();
                        eprintln!(
                            "Metadata update failed: scan_id={scan_id}, error={message}"
                        );
                        if let Err(database_error) = failure_database.fail_scan_run(
                            scan_id,
                            &message,
                        ) {
                            eprintln!(
                                "Unable to record metadata failure: scan_id={scan_id}, error={database_error}"
                            );
                        }

                        state.set_metadata_state(
                            MetadataState::Error(
                                message,
                            ),
                        );
                    }
                    Err(error) => {
                        let message = format!("Metadata worker failed: {error}");
                        eprintln!(
                            "Metadata update failed: scan_id={scan_id}, error={message}"
                        );
                        if let Err(database_error) = failure_database.fail_scan_run(scan_id, &message) {
                            eprintln!(
                                "Unable to record metadata failure: scan_id={scan_id}, error={database_error}"
                            );
                        }
                        state.set_metadata_state(MetadataState::Error(message));
                    }
                }

                state.finish_metadata_job();
            },
        );

        Ok(
            (),
        )
    }

    fn set_metadata_state(
        &self,
        new_state: MetadataState,
    ) {
        if let Ok(
            mut state,
        ) = self.metadata.write()
        {
            *state = new_state;
        }
    }

    fn finish_metadata_job(
        &self,
    ) {
        if let Ok(
            mut active,
        ) = self.metadata_job_active.lock()
        {
            *active = false;
        }
    }

    pub fn request_shutdown(
        &self,
    ) {
        let _ = self.shutdown_tx.send(
            true,
        );
    }

    pub fn shutdown_receiver(
        &self,
    ) -> watch::Receiver<bool> {
        self.shutdown_tx.subscribe()
    }

    pub fn stop_rclone_gui(
        &self,
    ) {
        let child = match self.rclone_gui.lock() {
            Ok(
                mut gui,
            ) => gui.take(),

            Err(
                error,
            ) => {
                eprintln!(
                    "Unable to access Rclone WebGUI process during shutdown: {error}"
                );

                return;
            }
        };

        if let Some(
            mut child,
        ) = child
        {
            if let Err(
                error,
            ) = rclone::gui::stop(
                &mut child,
            ) {
                eprintln!(
                    "Unable to stop Rclone WebGUI: {error}"
                );
            }
        }
    }
}

fn remote_state_mut(
    remotes: &mut GoogleRemotesState,
    kind: RemoteKind,
) -> &mut RemoteState {
    match kind {
        RemoteKind::MyDriveRw => &mut remotes.rw,
        RemoteKind::MyDriveRo => &mut remotes.ro,
    }
}
