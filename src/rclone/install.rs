use std::{
    env,
    error::Error,
    ffi::OsStr,
    fs::{
        self,
        File,
    },
    io,
    path::{
        Path,
        PathBuf,
    },
    process::Command,
    time::{
        SystemTime,
        UNIX_EPOCH,
    },
};

#[cfg(target_os = "linux")]
use std::process::Stdio;

use zip::ZipArchive;

use crate::bootstrap::Runtime;

use super::{
    command,
    executable_path,
};

/// Install the current Rclone release into BOREAL's private bin directory.
///
/// BOREAL does not:
///
/// - require administrator/root privileges
/// - install through a system package manager
/// - modify PATH
/// - modify a system Rclone installation
///
/// Installation location:
///
/// Linux/macOS:
///
///     ~/.boreal/bin/rclone
///
/// Windows:
///
///     %LOCALAPPDATA%\boreal\bin\rclone.exe
///
pub fn install(
    runtime: &Runtime,
) -> Result<PathBuf, Box<dyn Error>> {
    let destination = executable_path(
        runtime,
    )?;

    let bin_dir = destination
        .parent()
        .ok_or(
            "Unable to determine BOREAL bin directory",
        )?;

    fs::create_dir_all(
        bin_dir,
    )?;

    let platform = rclone_platform()?;

    let download_url = format!(
        "https://downloads.rclone.org/rclone-current-{platform}.zip"
    );

    let archive_path = temporary_archive_path();

    let extracted_path = temporary_executable_path(
        bin_dir,
    );

    println!(
        "==> Rclone is not installed"
    );

    println!(
        "==> Installing BOREAL-managed Rclone"
    );

    println!(
        "==> Platform: {platform}"
    );

    println!(
        "==> Downloading: {download_url}"
    );

    /*
     * Remove remnants from a previous interrupted installation.
     */
    remove_if_exists(
        &archive_path,
    )?;

    remove_if_exists(
        &extracted_path,
    )?;

    /*
     * Download the official current Rclone archive.
     */
    if let Err(error) = download(
        &download_url,
        &archive_path,
    ) {
        let _ = remove_if_exists(
            &archive_path,
        );

        let _ = remove_if_exists(
            &extracted_path,
        );

        return Err(
            error,
        );
    }

    println!(
        "==> Extracting Rclone"
    );

    /*
     * Extract only the Rclone executable from the ZIP archive.
     */
    if let Err(error) = extract_rclone(
        &archive_path,
        &extracted_path,
    ) {
        let _ = remove_if_exists(
            &archive_path,
        );

        let _ = remove_if_exists(
            &extracted_path,
        );

        return Err(
            error,
        );
    }

    /*
     * ZIP extraction does not necessarily preserve Unix executable
     * permissions.
     */
    set_executable_permissions(
        &extracted_path,
    )?;

    println!(
        "==> Verifying downloaded Rclone"
    );

    /*
     * Verify the temporary executable before replacing the managed
     * Rclone binary.
     */
    let version = command::version(
        &extracted_path,
    )?;

    println!(
        "==> Downloaded {version}"
    );

    /*
     * Replace any existing BOREAL-managed executable only after the
     * downloaded binary has successfully executed.
     */
    remove_if_exists(
        &destination,
    )?;

    fs::rename(
        &extracted_path,
        &destination,
    )
    .map_err(
        |error| {
            format!(
                "Unable to install Rclone to {}: {error}",
                destination.display()
            )
        },
    )?;

    /*
     * Clean up the downloaded archive.
     */
    remove_if_exists(
        &archive_path,
    )?;

    println!(
        "==> Rclone installed: {}",
        destination.display()
    );

    println!(
        "==> {version}"
    );

    Ok(
        destination,
    )
}

/// Determine the appropriate Rclone release platform identifier.
///
/// Supported BOREAL targets:
///
/// Linux:
///     x86_64  -> linux-amd64
///     aarch64 -> linux-arm64
///     arm     -> linux-arm-v7
///
/// macOS:
///     x86_64  -> osx-amd64
///     aarch64 -> osx-arm64
///
/// Windows:
///     x86_64  -> windows-amd64
///
fn rclone_platform(
) -> Result<&'static str, Box<dyn Error>> {
    let os = env::consts::OS;
    let arch = env::consts::ARCH;

    match (
        os,
        arch,
    ) {
        (
            "linux",
            "x86_64",
        ) => Ok(
            "linux-amd64",
        ),

        (
            "linux",
            "aarch64",
        ) => Ok(
            "linux-arm64",
        ),

        (
            "linux",
            "arm",
        ) => Ok(
            "linux-arm-v7",
        ),

        (
            "macos",
            "x86_64",
        ) => Ok(
            "osx-amd64",
        ),

        (
            "macos",
            "aarch64",
        ) => Ok(
            "osx-arm64",
        ),

        (
            "windows",
            "x86_64",
        ) => Ok(
            "windows-amd64",
        ),

        _ => Err(
            format!(
                "BOREAL does not currently support Rclone installation \
                 on OS '{os}' architecture '{arch}'"
            )
            .into(),
        ),
    }
}

/// Download the Rclone archive using a native platform downloader.
fn download(
    url: &str,
    destination: &Path,
) -> Result<(), Box<dyn Error>> {
    #[cfg(target_os = "windows")]
    {
        return download_windows(
            url,
            destination,
        );
    }

    #[cfg(target_os = "macos")]
    {
        return download_macos(
            url,
            destination,
        );
    }

    #[cfg(target_os = "linux")]
    {
        return download_linux(
            url,
            destination,
        );
    }

    #[allow(unreachable_code)]
    Err(
        "Unsupported operating system for Rclone download"
            .into(),
    )
}

#[cfg(target_os = "linux")]
fn download_linux(
    url: &str,
    destination: &Path,
) -> Result<(), Box<dyn Error>> {
    /*
     * Prefer curl when available.
     */
    if command_exists(
        "curl",
    ) {
        let status = Command::new(
            "curl",
        )
        .arg(
            "--fail",
        )
        .arg(
            "--location",
        )
        .arg(
            "--silent",
        )
        .arg(
            "--show-error",
        )
        .arg(
            "--output",
        )
        .arg(
            destination,
        )
        .arg(
            url,
        )
        .status()
        .map_err(
            |error| {
                format!(
                    "Unable to execute curl: {error}"
                )
            },
        )?;

        if status.success() {
            return Ok(
                (),
            );
        }

        eprintln!(
            "curl was unable to download Rclone; trying wget."
        );
    }

    /*
     * Fall back to wget.
     */
    if command_exists(
        "wget",
    ) {
        let status = Command::new(
            "wget",
        )
        .arg(
            "--quiet",
        )
        .arg(
            "--output-document",
        )
        .arg(
            destination,
        )
        .arg(
            url,
        )
        .status()
        .map_err(
            |error| {
                format!(
                    "Unable to execute wget: {error}"
                )
            },
        )?;

        if status.success() {
            return Ok(
                (),
            );
        }

        return Err(
            "wget was unable to download Rclone"
                .into(),
        );
    }

    Err(
        "BOREAL could not download Rclone because neither curl nor wget \
         is available"
            .into(),
    )
}

#[cfg(target_os = "macos")]
fn download_macos(
    url: &str,
    destination: &Path,
) -> Result<(), Box<dyn Error>> {
    /*
     * macOS provides /usr/bin/curl as part of the operating system.
     *
     * No Homebrew installation is required.
     */
    let curl = Path::new(
        "/usr/bin/curl",
    );

    if !curl.is_file() {
        return Err(
            "macOS system curl was not found at /usr/bin/curl"
                .into(),
        );
    }

    let status = Command::new(
        curl,
    )
    .arg(
        "--fail",
    )
    .arg(
        "--location",
    )
    .arg(
        "--silent",
    )
    .arg(
        "--show-error",
    )
    .arg(
        "--output",
    )
    .arg(
        destination,
    )
    .arg(
        url,
    )
    .status()
    .map_err(
        |error| {
            format!(
                "Unable to execute macOS curl: {error}"
            )
        },
    )?;

    if !status.success() {
        return Err(
            "curl was unable to download Rclone"
                .into(),
        );
    }

    Ok(
        (),
    )
}

#[cfg(target_os = "windows")]
fn download_windows(
    url: &str,
    destination: &Path,
) -> Result<(), Box<dyn Error>> {
    /*
     * Pass the URL and destination through environment variables rather
     * than interpolating them directly into PowerShell source.
     *
     * This avoids quoting problems with spaces and special characters in
     * Windows filesystem paths.
     */
    let script = concat!(
        "$ErrorActionPreference = 'Stop'; ",
        "$ProgressPreference = 'SilentlyContinue'; ",
        "Invoke-WebRequest ",
        "-Uri $env:BOREAL_RCLONE_URL ",
        "-OutFile $env:BOREAL_RCLONE_DEST"
    );

    let status = Command::new(
        "powershell.exe",
    )
    .arg(
        "-NoLogo",
    )
    .arg(
        "-NoProfile",
    )
    .arg(
        "-NonInteractive",
    )
    .arg(
        "-ExecutionPolicy",
    )
    .arg(
        "Bypass",
    )
    .arg(
        "-Command",
    )
    .arg(
        script,
    )
    .env(
        "BOREAL_RCLONE_URL",
        url,
    )
    .env(
        "BOREAL_RCLONE_DEST",
        destination,
    )
    .status()
    .map_err(
        |error| {
            format!(
                "Unable to start Windows PowerShell to download Rclone: {error}"
            )
        },
    )?;

    if !status.success() {
        return Err(
            "PowerShell was unable to download Rclone"
                .into(),
        );
    }

    Ok(
        (),
    )
}

/// Extract only rclone/rclone.exe from the downloaded ZIP archive.
fn extract_rclone(
    archive_path: &Path,
    destination: &Path,
) -> Result<(), Box<dyn Error>> {
    let archive_file = File::open(
        archive_path,
    )
    .map_err(
        |error| {
            format!(
                "Unable to open downloaded Rclone archive {}: {error}",
                archive_path.display()
            )
        },
    )?;

    let mut archive = ZipArchive::new(
        archive_file,
    )
    .map_err(
        |error| {
            format!(
                "Unable to read Rclone ZIP archive {}: {error}",
                archive_path.display()
            )
        },
    )?;

    let expected_name = if cfg!(windows) {
        OsStr::new(
            "rclone.exe",
        )
    } else {
        OsStr::new(
            "rclone",
        )
    };

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(
                index,
            )
            .map_err(
                |error| {
                    format!(
                        "Unable to read Rclone ZIP entry {index}: {error}"
                    )
                },
            )?;

        /*
         * Rclone archives contain a versioned directory such as:
         *
         *     rclone-v1.75.0-linux-amd64/rclone
         *
         * We only need the executable itself.
         */
        let Some(entry_path) = entry.enclosed_name() else {
            continue;
        };

        let Some(file_name) = entry_path.file_name() else {
            continue;
        };

        if file_name != expected_name {
            continue;
        }

        let mut output = File::create(
            destination,
        )
        .map_err(
            |error| {
                format!(
                    "Unable to create temporary Rclone executable {}: {error}",
                    destination.display()
                )
            },
        )?;

        io::copy(
            &mut entry,
            &mut output,
        )
        .map_err(
            |error| {
                format!(
                    "Unable to extract Rclone executable: {error}"
                )
            },
        )?;

        return Ok(
            (),
        );
    }

    Err(
        format!(
            "Rclone executable was not found inside {}",
            archive_path.display()
        )
        .into(),
    )
}

#[cfg(unix)]
fn set_executable_permissions(
    path: &Path,
) -> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(
        path,
    )?
    .permissions();

    permissions.set_mode(
        0o755,
    );

    fs::set_permissions(
        path,
        permissions,
    )?;

    Ok(
        (),
    )
}

#[cfg(windows)]
fn set_executable_permissions(
    _path: &Path,
) -> Result<(), Box<dyn Error>> {
    /*
     * Windows does not use Unix executable permission bits.
     */
    Ok(
        (),
    )
}

/// Test whether a command can be launched.
///
/// This is used only to select the Linux downloader. It is not used to
/// discover Rclone.
#[cfg(target_os = "linux")]
fn command_exists(
    command: &str,
) -> bool {
    Command::new(
        command,
    )
    .arg(
        "--version",
    )
    .stdout(
        Stdio::null(),
    )
    .stderr(
        Stdio::null(),
    )
    .status()
    .is_ok()
}

/// Generate a unique temporary ZIP archive path.
fn temporary_archive_path(
) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(
            UNIX_EPOCH,
        )
        .unwrap_or_default()
        .as_nanos();

    env::temp_dir().join(
        format!(
            "boreal-rclone-{}-{timestamp}.zip",
            std::process::id(),
        ),
    )
}

/// Generate a temporary executable path inside BOREAL's bin directory.
///
/// Keeping this file on the same filesystem as the final executable allows
/// the final rename to remain a simple local filesystem operation.
fn temporary_executable_path(
    bin_dir: &Path,
) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(
            UNIX_EPOCH,
        )
        .unwrap_or_default()
        .as_nanos();

    let name = if cfg!(windows) {
        format!(
            ".rclone-{timestamp}.tmp.exe"
        )
    } else {
        format!(
            ".rclone-{timestamp}.tmp"
        )
    };

    bin_dir.join(
        name,
    )
}

/// Remove a file if it exists.
fn remove_if_exists(
    path: &Path,
) -> Result<(), Box<dyn Error>> {
    match fs::remove_file(
        path,
    ) {
        Ok(
            (),
        ) => Ok(
            (),
        ),

        Err(
            error,
        ) if error.kind() == io::ErrorKind::NotFound => {
            Ok(
                (),
            )
        }

        Err(
            error,
        ) => Err(
            format!(
                "Unable to remove {}: {error}",
                path.display()
            )
            .into(),
        ),
    }
}