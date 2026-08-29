use std::{
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
    google::{
        self,
        client::GoogleClientConfig,
    },
    rclone::{
        self,
        RcloneStatus,
    },
};

pub struct AppState {
    pub runtime: Runtime,

    pub rclone: RwLock<RcloneState>,

    pub google_client: RwLock<GoogleClientState>,

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

        Self {
            runtime,

            rclone: RwLock::new(
                RcloneState::Initializing,
            ),

            google_client: RwLock::new(
                google_client,
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
