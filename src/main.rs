mod bootstrap;
mod config;
mod web;

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let runtime = bootstrap::initialize()?;

    println!("BOREAL initialized.");
    println!("BOREAL home: {}", runtime.boreal_home.display());
    println!("Configured directories:");

    for (name, path) in &runtime.directories {
        println!("  {:<12} {}", name, path.display());
    }

    Ok(())
}