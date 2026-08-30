use std::{
    env,
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
    RcloneError,
};

/// Install the current Rclone release into BOREAL's
/// private user-local bin directory.
///
/// No administrator/root permissions are required.
///
/// BOREAL does not:
///
/// - use a system package manager
/// - modify PATH
/// - modify a system Rclone installation
pub fn install(
    runtime: &Runtime,
) -> Result<PathBuf, RcloneError> {
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

    remove_if_exists(
        &archive_path,
    )?;

    remove_if_exists(
        &extracted_path,
    )?;

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

    set_executable_permissions(
        &extracted_path,
    )?;

    println!(
        "==> Verifying downloaded Rclone"
    );

    let version = command::version(
        &extracted_path,
    )?;

    println!(
        "==> Downloaded {version}"
    );

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

fn rclone_platform(
) -> Result<&'static str, RcloneError> {
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

fn download(
    url: &str,
    destination: &Path,
) -> Result<(), RcloneError> {
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
) -> Result<(), RcloneError> {
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
) -> Result<(), RcloneError> {
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
) -> Result<(), RcloneError> {
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

fn extract_rclone(
    archive_path: &Path,
    destination: &Path,
) -> Result<(), RcloneError> {
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
) -> Result<(), RcloneError> {
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
) -> Result<(), RcloneError> {
    Ok(
        (),
    )
}

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

fn temporary_archive_path() -> PathBuf {
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

fn remove_if_exists(
    path: &Path,
) -> Result<(), RcloneError> {
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
