# BOREAL

**Browser-based Organizer for Rclone Exploration, Audit & Lookup**

BOREAL is a local desktop application for inventorying, exploring, and auditing
Google Drive content through Rclone. It indexes My Drive, Shared with me, and
Shared Drives into a private local SQLite database for faster exploration and
analysis.

> **BOREAL v0.1.3 is beta software.** Back up important data and review the
> known limitations before use.

| Attribute | Value |
| --- | --- |
| **Project** | Boreal |
| **Description** | Browser-based Organizer for Rclone Exploration, Audit and Lookup |
| **Author** | John Haverlack |
| **License** | MIT |
| **Version** | 0.1.3 |
| **Maturity** | BETA |
| **Date** | 2026-09-01 |

> AI Attestation: Generative AI was used in the development of this codebase.
> The architecture and design goals are those of the author.

## Download

Download the binary matching your operating system and processor from the
[BOREAL v0.1.3 release](https://github.com/jehaverlack/uaf-boreal/releases/tag/v0.1.3).

| System | Processor | Download |
| --- | --- | --- |
| Linux | x86_64 / AMD64 | [boreal-v0.1.3-linux-x86_64](https://github.com/jehaverlack/uaf-boreal/raw/refs/tags/v0.1.3/dist/boreal-v0.1.3-linux-x86_64) |
| Linux | ARM64 / AArch64 | [boreal-v0.1.3-linux-aarch64](https://github.com/jehaverlack/uaf-boreal/raw/refs/tags/v0.1.3/dist/boreal-v0.1.3-linux-aarch64) |
| Linux | ARMv7 32-bit | [boreal-v0.1.3-linux-armv7](https://github.com/jehaverlack/uaf-boreal/raw/refs/tags/v0.1.3/dist/boreal-v0.1.3-linux-armv7) |
| Windows | x86_64 / AMD64 | [boreal-v0.1.3-windows-x86_64.exe](https://github.com/jehaverlack/uaf-boreal/raw/refs/tags/v0.1.3/dist/boreal-v0.1.3-windows-x86_64.exe) |
| macOS | Apple Silicon / ARM64 | [boreal-v0.1.3-macos-aarch64](https://github.com/jehaverlack/uaf-boreal/raw/refs/tags/v0.1.3/dist/boreal-v0.1.3-macos-aarch64) |
| macOS | Intel x86_64 | [boreal-v0.1.3-macos-x86_64](https://github.com/jehaverlack/uaf-boreal/raw/refs/tags/v0.1.3/dist/boreal-v0.1.3-macos-x86_64) |

The release also provides
[SHA256SUMS](https://github.com/jehaverlack/uaf-boreal/raw/refs/tags/v0.1.3/dist/SHA256SUMS)
for verifying downloads.

### Identify Your Processor

On Linux:

```bash
uname -m
```

- `x86_64`: use the Linux x86_64 download.
- `aarch64` or `arm64`: use the Linux ARM64 download.
- `armv7l`: use the Linux ARMv7 download.

On macOS, choose **About This Mac** from the Apple menu:

- Apple M-series processors use the macOS Apple Silicon download.
- Intel processors use the macOS Intel download.

## Screenshot

![Boreal Dashboard](docs/boreal-dashboard.png)

## What BOREAL Does

BOREAL currently provides:

- A local-only browser interface backed by a standalone Rust application.
- Automatic installation and management of a private Rclone executable.
- Guided Google OAuth Desktop Client ID and read-only remote setup.
- Separate inventories for My Drive, Shared with me, and Shared Drives.
- Local SQLite storage for file, folder, ownership, permission, and scan data.
- Search, filtering, tags, identity-directory correlation, and recursive folder
  size analysis.
- Manual metadata updates with background progress reporting.

BOREAL does not modify the user's PATH and does not require administrator or
root privileges for normal use. The WebUI listens only on the local machine.

BOREAL is not a replacement for Rclone. Rclone performs Google Drive access;
BOREAL adds a local administrative, inventory, and audit interface.

## Use Cases

BOREAL can help answer questions such as:

- Which files in **Shared with me** belong to former users and may be at risk
  of being purged?
- Which large files in **My Drive** can be migrated or deleted?
- If I am leaving UAF or a department, which permissions should be transferred
  or removed?
- Which permissions held by former users still need to be removed?
- Which **My Drive** or **Shared Drive** documents need to be handed off to
  someone else?
- Which **My Drive** documents should be moved to a **Shared Drive**?

## Run BOREAL

### Linux

After downloading the correct binary:

```bash
cd ~/Downloads
chmod +x boreal-v0.1.3-linux-x86_64
./boreal-v0.1.3-linux-x86_64
```

Substitute the ARM64 or ARMv7 filename when appropriate.

### Windows

1. Download `boreal-v0.1.3-windows-x86_64.exe`.
2. Open the downloaded file.
3. Keep the BOREAL console window open while using the application.

Windows may display a warning because the beta executable is not code-signed.
Verify that it came from the official BOREAL release and that its checksum
matches before running it.

### macOS

After downloading the correct binary:

```bash
cd ~/Downloads
chmod +x boreal-v0.1.3-macos-aarch64
./boreal-v0.1.3-macos-aarch64
```

Substitute the Intel filename on an Intel Mac. macOS may display a warning
because the beta executable is not code-signed.

See [Running BOREAL on macOS](docs/MACOS.md) for the complete executable
permission and Gatekeeper approval procedure. Do not run BOREAL with `sudo`.

## First-Run Setup

When BOREAL starts:

1. It creates its private runtime directory.
2. It downloads, installs, and verifies its managed Rclone executable.
3. It starts the local WebUI and opens the default browser.
4. The dashboard guides the user through importing a Google OAuth Desktop
   Client ID.
5. The user authorizes the read-only `my-drive-ro` Rclone remote in Google.
6. The user may optionally configure an organizational directory spreadsheet.
7. The user starts a metadata update to inventory accessible Drive content.

Initial Rclone installation and metadata collection require internet access.
Large Google Drive inventories can take time to complete.

Use **App → Quit BOREAL** in the WebUI or press **Ctrl-C** in the terminal to
stop the application. Closing only the browser tab does not stop BOREAL.

## Local Data

BOREAL keeps its runtime files separate from system Rclone configuration:

| Platform | BOREAL home |
| --- | --- |
| Linux and macOS | `~/.boreal` |
| Windows | `%LOCALAPPDATA%\boreal` |

The runtime tree contains:

```text
.boreal/
├── bin/          BOREAL-managed Rclone executable
├── conf/         BOREAL, Google OAuth, and Rclone configuration
├── data/sqlite/  Local inventory database
└── logs/         Daily application logs
```

Google OAuth credentials and Rclone tokens are stored locally in the BOREAL
configuration directory. BOREAL does not modify the user's PATH.

## Beta Limitations

- Migration and copy execution are not implemented.
- BOREAL does not create Shared Drives.
- BOREAL does not automatically delete, trash, or move Google Drive content.
- Published beta binaries are not currently code-signed.
- Google Cloud and Google Auth Platform setup is still required to create a
  Desktop OAuth Client ID.

See [DESIGN.md](docs/DESIGN.md) for architecture, security boundaries, and
planned development.

## Build from Source

BOREAL is written in Rust. Build-host setup scripts are provided for developers:

```bash
./tools/setup-build-linux.sh
./tools/setup-build-macos.sh
```

Create enabled release-target binaries with:

```bash
./tools/build-release.sh
```

Linux builds Linux and Windows targets. macOS targets must be built on macOS.
After copying all enabled target binaries into `build/`, stage the complete
release set and generate checksums with:

```bash
./tools/stage-release.sh
```

See [tools/WORKFLOW.md](tools/WORKFLOW.md) for the complete release process.

## License

BOREAL is distributed under the [MIT License](LICENSE).
