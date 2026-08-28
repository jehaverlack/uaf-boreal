mod app;
mod bootstrap;
mod config;
mod google;
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

    println!(
        "BOREAL initialized."
    );

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

    let state = Arc::new(
        AppState::new(
            runtime,
        ),
    );

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