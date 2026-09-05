use std::{
    io::{BufRead, BufReader, Read},
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

use crate::bootstrap::Runtime;

use super::{RcloneError, config};

/// Start Rclone's WebGUI as a child of BOREAL.
pub fn start(runtime: &Runtime, executable: &Path) -> Result<(Child, String), RcloneError> {
    let config_path = config::path(runtime)?;

    let mut child = Command::new(executable)
        .args(["gui", "--no-open-browser", "--config"])
        .arg(config_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Unable to start Rclone WebGUI: {error}"))?;

    let (url_tx, url_rx) = mpsc::channel();

    if let Some(stdout) = child.stdout.take() {
        read_output(stdout, url_tx.clone());
    }

    if let Some(stderr) = child.stderr.take() {
        read_output(stderr, url_tx);
    }

    let mut output = Vec::new();
    let gui_url = loop {
        match url_rx.recv_timeout(Duration::from_secs(60)) {
            Ok(line) => {
                if let Some((_, url)) = line.split_once("GUI available at ") {
                    break url.trim().to_string();
                }
                println!("{line}");
                output.push(line);
                if output.len() > 12 {
                    output.remove(0);
                }
            }
            Err(error) => {
                let exit = child.try_wait().ok().flatten();
                let _ = child.kill();
                let _ = child.wait();
                let detail = if output.is_empty() {
                    format!("no diagnostic output; readiness channel closed: {error}")
                } else {
                    output.join(" | ")
                };
                return Err(format!(
                    "Rclone WebGUI exited before becoming ready{}: {detail}",
                    exit.map(|status| format!(" ({status})"))
                        .unwrap_or_default()
                )
                .into());
            }
        }
    };

    println!("Rclone WebGUI is ready.");

    Ok((child, gui_url))
}

fn read_output<R>(output: R, url_tx: mpsc::Sender<String>)
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        for line in BufReader::new(output).lines() {
            let Ok(line) = line else {
                break;
            };

            if url_tx.send(line).is_err() {
                break;
            }
        }
    });
}

/// Stop the managed Rclone WebGUI process and reap it.
pub fn stop(child: &mut Child) -> Result<(), RcloneError> {
    if child.try_wait()?.is_none() {
        child.kill()?;
        child.wait()?;
    }

    println!("Rclone WebGUI stopped.");

    Ok(())
}
