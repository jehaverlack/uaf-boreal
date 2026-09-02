# Upgrading BOREAL

BOREAL checks its published changelog for newer releases, but it does not
replace the running executable automatically. Upgrading the executable does not
remove the configuration, Google credentials, Rclone configuration, inventory,
tags, migration history, or logs stored in the user-specific BOREAL home.

## 1. Check for a New Version

Open **App → Update** and select **Check for updates**. If an update is
available, use **Download for this system**, or download the correct binary from
the project's release page. Verify the file against the release
`SHA256SUMS` before running it.

## 2. Finish Active Work and Quit

Wait for metadata updates, migrations, and downloads to finish. Then use
**App → Quit BOREAL** or the desktop tray/menu-bar **Quit BOREAL** action.
BOREAL warns before interrupting active work.

Do not start the new executable while the old version is still running. A
second launch normally opens the existing WebUI and exits.

## 3. Install or Place the New Binary

BOREAL is currently a portable, versioned executable rather than an installed
application. You may keep the old binary temporarily as a rollback copy.

- **Linux/macOS:** make the new download executable with `chmod +x`, then run
  it from the desired directory.
- **Windows:** close BOREAL before moving or renaming the old `.exe`. Place the
  new `.exe` where you want to keep it, then double-click it.

The detailed platform steps are in [Install-Linux.md](Install-Linux.md),
[Install-MACOS.md](Install-MACOS.md), and
[Install-Windows.md](Install-Windows.md).

## 4. Verify the Upgrade

Start the new executable and confirm the installed version under
**App → Update**. Existing settings and data should appear automatically.
BOREAL applies forward-only SQLite schema migrations at startup when required.

Keep a backup of the BOREAL home before a major upgrade if the local inventory
or annotations are important. Do not copy an older database over a database
that has already been opened by a newer release.

Automatic staged upgrades and per-user application installation are planned,
but are not part of the current release.
