use std::{
    io::{
        BufRead,
        BufReader,
        Read,
    },
    path::Path,
    process::{
        Child,
        Command,
        Stdio,
    },
    sync::mpsc,
    thread,
    time::Duration,
};

use crate::bootstrap::Runtime;

use super::{
    config,
    RcloneError,
};

/// Start Rclone's WebGUI as a child of BOREAL.
pub fn start(
    runtime: &Runtime,
    executable: &Path,
) -> Result<(Child, String), RcloneError> {
    let config_path = config::path(
        runtime,
    )?;

    let mut child = Command::new(
        executable,
    )
    .args([
        "gui",
        "--no-open-browser",
        "--addr=127.0.0.1:5572",
        "--api-addr=127.0.0.1:5573",
        "--config",
    ])
    .arg(
        config_path,
    )
    .stdout(
        Stdio::piped(),
    )
    .stderr(
        Stdio::piped(),
    )
    .spawn()
    .map_err(
        |error| {
            format!(
                "Unable to start Rclone WebGUI: {error}"
            )
        },
    )?;

    let (
        url_tx,
        url_rx,
    ) = mpsc::channel();

    if let Some(
        stdout,
    ) = child.stdout.take()
    {
        read_output(
            stdout,
            url_tx.clone(),
        );
    }

    if let Some(
        stderr,
    ) = child.stderr.take()
    {
        read_output(
            stderr,
            url_tx,
        );
    }

    let gui_url = match url_rx.recv_timeout(
        Duration::from_secs(
            60,
        ),
    ) {
        Ok(
            url,
        ) => url,

        Err(
            error,
        ) => {
            let _ = child.kill();
            let _ = child.wait();

            return Err(
                format!(
                    "Rclone WebGUI did not become ready: {error}"
                )
                .into(),
            );
        }
    };

    println!(
        "Rclone WebGUI is ready."
    );

    Ok(
        (
            child,
            gui_url,
        ),
    )
}

fn read_output<R>(
    output: R,
    url_tx: mpsc::Sender<String>,
)
where
    R: Read + Send + 'static,
{
    thread::spawn(
        move || {
            for line in BufReader::new(
                output,
            )
            .lines()
            {
                let Ok(
                    line,
                ) = line
                else {
                    break;
                };

                if let Some(
                    (_, url),
                ) = line.split_once(
                    "GUI available at ",
                ) {
                    let _ = url_tx.send(
                        url.trim().to_string(),
                    );

                    continue;
                }

                println!(
                    "{line}"
                );
            }
        },
    );
}

/// Stop the managed Rclone WebGUI process and reap it.
pub fn stop(
    child: &mut Child,
) -> Result<(), RcloneError> {
    if child.try_wait()?.is_none() {
        child.kill()?;
        child.wait()?;
    }

    println!(
        "Rclone WebGUI stopped."
    );

    Ok(
        (),
    )
}
