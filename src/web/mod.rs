pub mod routes;

use crate::config;
use axum::Router;
use serde_json::Value;
use std::error::Error;

pub async fn run(
    boreal: &Value,
) -> Result<(), Box<dyn Error>> {
    let web_config =
        config::get_webapp_config(boreal)?;

    let address = format!(
        "{}:{}",
        web_config.listen,
        web_config.port
    );

    /*
     * BOREAL is intentionally a local desktop application.
     *
     * For now, refuse to bind to anything except localhost.
     * We can revisit this policy later if remote access is
     * ever intentionally supported.
     */
    if web_config.listen != "127.0.0.1"
        && web_config.listen != "localhost"
        && web_config.listen != "::1"
    {
        return Err(
            format!(
                "BOREAL WebUI must listen on localhost; \
                 configured address is '{}'",
                web_config.listen
            )
            .into(),
        );
    }

    let app = Router::new()
        .merge(routes::router());

    let listener =
        tokio::net::TcpListener::bind(&address)
            .await?;

    println!();
    println!(
        "BOREAL WebUI: http://{}:{}",
        web_config.listen,
        web_config.port
    );

    println!("Press Ctrl-C to stop BOREAL.");

    axum::serve(
        listener,
        app,
    )
    .await?;

    Ok(())
}