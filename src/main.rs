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

    println!("Configured directories:");

    for (name, path) in &runtime.directories {
        println!(
            "  {:<12} {}",
            name,
            path.display()
        );
    }

    /*
     * Initialize application services.
     *
     * Individual service failures are stored in AppState rather
     * than terminating BOREAL.
     */
    let state = Arc::new(
        AppState::initialize(
            runtime,
        ),
    );

    web::run(
        state,
    )
    .await?;

    Ok(())
}