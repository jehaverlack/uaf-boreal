macro_rules! println {
    () => {
        std::println!()
    };

    ($($argument:tt)*) => {
        log::info!($($argument)*)
    };
}

macro_rules! eprintln {
    ($($argument:tt)*) => {
        log::error!($($argument)*)
    };
}

mod app;
mod bootstrap;
mod config;
mod database;
mod google;
mod logging;
mod rclone;
mod update;
mod web;

use std::{error::Error, sync::Arc};

use app::AppState;

// Native macOS folder dialogs in this non-windowed application must be opened
// from the main OS thread. Blocking Rclone and database jobs are still sent to
// Tokio's blocking worker pool.
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let runtime = bootstrap::initialize()?;

    logging::initialize(&runtime)?;

    println!("BOREAL initialized.");

    println!("BOREAL home: {}", runtime.boreal_home.display());

    println!("Configured directories:");

    for (name, path) in &runtime.directories {
        println!("  {:<12} {}", name, path.display());
    }

    let state = Arc::new(AppState::new(runtime));

    AppState::initialize_rclone(Arc::clone(&state));
    AppState::check_for_updates(Arc::clone(&state));

    let web_result = web::run(Arc::clone(&state)).await;

    state.request_shutdown();

    state.stop_rclone_gui();

    web_result?;

    Ok(())
}
