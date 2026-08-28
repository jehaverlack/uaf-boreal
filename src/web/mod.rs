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

    /*
     * BOREAL is a local desktop application.
     *
     * Do not permit accidental exposure of the WebUI
     * on external interfaces.
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

    let address = format!(
        "{}:{}",
        web_config.listen,
        web_config.port
    );

    let url = format!(
        "http://{}:{}",
        web_config.listen,
        web_config.port
    );

    /*
     * Build the application router.
     */
    let app = Router::new()
        .merge(
            routes::router()
        );

    /*
     * Bind first.
     *
     * If the configured address or port cannot be
     * used, fail before opening a browser.
     */
    let listener =
        tokio::net::TcpListener::bind(
            &address
        )
        .await?;

    println!();
    println!(
        "BOREAL WebUI: {url}"
    );

    /*
     * Open the user's default browser when enabled.
     *
     * Browser launch failure is not fatal. The
     * application can still be reached manually.
     */
    if web_config.open_browser {
        println!(
            "Opening default browser..."
        );

        if let Err(error) =
            webbrowser::open(&url)
        {
            eprintln!(
                "Unable to open default browser: {error}"
            );

            eprintln!(
                "Open BOREAL manually at: {url}"
            );
        }
    }

    println!(
        "Press Ctrl-C to stop BOREAL."
    );

    /*
     * Start the Axum HTTP server.
     */
    axum::serve(
        listener,
        app,
    )
    .await?;

    Ok(())
}