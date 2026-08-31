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
mod web;

use std::{error::Error, sync::Arc};

use app::AppState;

#[tokio::main]
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

    state.wait_for_initialization().await;

    println!("BOREAL initialization checks completed.");

    let web_result = web::run(Arc::clone(&state)).await;

    state.request_shutdown();

    state.stop_rclone_gui();

    web_result?;

    Ok(())
}
