mod app;
mod bootstrap;
mod config;
mod rclone;
mod web;

use std::{
    error::Error,
    sync::Arc,
};

use app::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let runtime = bootstrap::initialize()?;

    println!("BOREAL initialized.");

    println!(
        "BOREAL home: {}",
        runtime.boreal_home.display()
    );

    println!(
        "Configured directories:"
    );

    for (name, path) in &runtime.directories {
        println!(
            "  {:<12} {}",
            name,
            path.display()
        );
    }

    /*
     * Create shared application state.
     *
     * Supporting services such as Rclone initialize independently so that
     * failures do not prevent the BOREAL WebUI from starting.
     */
    let state = Arc::new(
        AppState::new(
            runtime,
        ),
    );

    /*
     * Begin Rclone initialization in the background.
     *
     * The WebUI starts immediately and can display the Initializing state
     * while Rclone is being downloaded, installed, and verified.
     */
    AppState::initialize_rclone(
        Arc::clone(
            &state,
        ),
    );

    web::run(
        state,
    )
    .await?;

    Ok(())
}