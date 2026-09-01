# BOREAL

**Browser-based Organizer for Rclone Exploration, Audit & Lookup**

| Attribute | Value |
| --- | --- |
| **Project** | Boreal |
| **Description** | Browser-based Organizer for Rclone Exploration, Audit and Lookup |
| **Author** | John Haverlack |
| **License** | MIT |
| **Version** | 0.1.4 |
| **Maturity** | BETA |
| **Date** | 2026-09-01 |

> AI Attestation: Generative AI was used in the development of this codebase.
> The architecture and design goals are those of the author.

BOREAL is a local desktop application for inventorying, exploring, and auditing
Google Drive. It uses Rclone to collect metadata from My Drive, Shared with me,
and Shared Drives, then stores that inventory in a private local database for
fast searching and analysis.

BOREAL runs on Linux, Windows, and macOS. Its browser interface is available
only on the local computer, and normal use does not require administrator or
root privileges.


## Why Use BOREAL?

BOREAL makes large Google Drive accounts easier to understand. It provides one
place to explore files, folders, ownership, sharing permissions, sizes, and
locally assigned tags without repeatedly querying Google Drive.

BOREAL can help answer questions such as:

- Which files shared with me belong to former users and may be at risk of being
  purged?
- Which large My Drive files can be migrated or deleted?
- Which permissions should be transferred or removed when someone leaves?
- Which permissions held by former users still need attention?
- Which documents need to be handed off to another person?
- Which My Drive documents should be moved to a Shared Drive?

BOREAL manages its own private Rclone installation, does not modify the user's
`PATH`, and does not automatically delete or move Google Drive content.

## Dashboard Screenshot

![BOREAL dashboard](docs/boreal-dashboard.png)

## Install BOREAL

Download the binary matching your operating system and processor from the
[BOREAL v0.1.4 release](https://github.com/jehaverlack/uaf-boreal/releases/tag/v0.1.4).

| System | Processor | Download | Instructions |
| --- | --- | --- | --- |
| Linux | x86_64 / AMD64 | [Download](https://github.com/jehaverlack/uaf-boreal/raw/refs/tags/v0.1.4/dist/boreal-v0.1.4-linux-x86_64) | [Install on Linux](docs/Install-Linux.md) |
| Linux | ARM64 / AArch64 | [Download](https://github.com/jehaverlack/uaf-boreal/raw/refs/tags/v0.1.4/dist/boreal-v0.1.4-linux-aarch64) | [Install on Linux](docs/Install-Linux.md) |
| Linux | ARMv7 32-bit | [Download](https://github.com/jehaverlack/uaf-boreal/raw/refs/tags/v0.1.4/dist/boreal-v0.1.4-linux-armv7) | [Install on Linux](docs/Install-Linux.md) |
| Windows | x86_64 / AMD64 | [Download](https://github.com/jehaverlack/uaf-boreal/raw/refs/tags/v0.1.4/dist/boreal-v0.1.4-windows-x86_64.exe) | [Install on Windows](docs/Install-Windows.md) |
| macOS | Apple Silicon / ARM64 | [Download](https://github.com/jehaverlack/uaf-boreal/raw/refs/tags/v0.1.4/dist/boreal-v0.1.4-macos-aarch64) | [Install on macOS](docs/Install-MACOS.md) |
| macOS | Intel x86_64 | [Download](https://github.com/jehaverlack/uaf-boreal/raw/refs/tags/v0.1.4/dist/boreal-v0.1.4-macos-x86_64) | [Install on macOS](docs/Install-MACOS.md) |

Use the release
[SHA256SUMS](https://github.com/jehaverlack/uaf-boreal/raw/refs/tags/v0.1.4/dist/SHA256SUMS)
file to verify your download.

## Configure BOREAL

Start BOREAL and follow the Setup Progress checklist in the browser:

1. Allow BOREAL to download and verify its private Rclone executable.
2. Create or import a Google OAuth Desktop Client ID using the guided setup.
3. Authorize BOREAL's read-only My Drive remote in Google.
4. Optionally link a Google Sheets persons directory.
5. Run the initial metadata update.

The initial Rclone installation and metadata update require internet access.
Large Drive inventories, especially Shared Drives, may take several hours.

Use **App → Quit BOREAL** in the browser interface or press **Ctrl-C** in the
terminal to stop BOREAL. Closing only the browser tab does not stop the
application.

Architecture and security details are available in
[DESIGN.md](docs/DESIGN.md). BOREAL is distributed under the
[MIT License](LICENSE).
