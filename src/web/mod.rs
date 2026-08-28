pub mod routes;

use std::{
    error::Error,
    sync::Arc,
};

use axum::Router;

use crate::{
    app::AppState,
    config,
};

/// Run the local BOREAL WebUI.
pub async fn run(
    state: Arc<AppState>,
) -> Result<(), Box<dyn Error>> {
    let webapp = config::get_webapp_config(
        &state.runtime.boreal,
    )?;

    /*
     * BOREAL's WebUI must remain local-only.
     */
    match webapp.listen.as_str() {
        "127.0.0.1"
        | "localhost"
        | "::1" => {}

        other => {
            return Err(
                format!(
                    "BOREAL refuses to listen on non-local address: {other}"
                )
                .into(),
            );
        }
    }

    let bind_address = format!(
        "{}:{}",
        webapp.listen,
        webapp.port,
    );

    /*
     * Bind before opening the browser.
     *
     * This ensures that the browser is not opened unless the WebUI
     * successfully acquires its configured listening port.
     */
    let listener = tokio::net::TcpListener::bind(
        &bind_address,
    )
    .await?;

    let app: Router = routes::router()
        .with_state(
            state,
        );

    let browser_host = match webapp.listen.as_str() {
        "::1" => "[::1]",
        other => other,
    };

    let url = format!(
        "http://{}:{}",
        browser_host,
        webapp.port,
    );

    println!(
        "BOREAL WebUI: {url}"
    );

    if webapp.open_browser {
        if let Err(error) = webbrowser::open(
            &url,
        ) {
            eprintln!(
                "Unable to open default browser: {error}"
            );

            eprintln!(
                "Open this URL manually: {url}"
            );
        }
    }

    println!(
        "Press Ctrl-C to stop BOREAL."
    );

    axum::serve(
        listener,
        app,
    )
    .await?;

    Ok(())
}