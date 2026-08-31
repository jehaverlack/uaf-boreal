# Boreal - uaf-boreal
Browser-based Organizer for Rclone Exploration, Audit & Lookup

| Attribute | Value |
| --- | --- |
| **Project** | Boreal |
| **Description** | Browser-based Organizer for Rclone Exploration, Audit and Lookup | 
| **Author** | John Haverlack |
| **License** | MIT |
| **Version** | 0.0.2 |
| **Date** | 2026-08-27 |

> AI Attestation: Generative AI was used for there development of this code base.  The architecture and design goals for this code are that of the author.

## Overview

BOREAL — **Browser-based Organizer for Rclone Exploration, Audit & Lookup** — is a cross-platform desktop application intended to provide a graphical interface for configuring, exploring, auditing, and migrating data accessible through Rclone.

BOREAL is designed as a standalone Rust application with a local browser-based user interface. The application runs a web server on the local system and opens the user's default browser to the BOREAL interface. The goal is to provide a consistent interface across Linux, macOS, and Windows while retaining Rclone as the underlying data-access and transfer engine.

The initial focus is Google Drive administration and migration. BOREAL is intended to help users:

- Detect and configure Rclone.
- Configure Google OAuth Client IDs and Rclone remotes.
- Discover and inventory **My Drive**, **Shared with me**, and **Shared Drives**.
- Collect file and folder metadata, including ownership, permissions, and sharing information.
- Calculate and retain recursive folder sizes rather than requiring repeated on-demand size queries.
- Search and explore indexed Drive content.
- Identify files and folders that should be migrated.
- Build and track migration plans.
- Support migration of content into Google Shared Drives.
- Eventually assist with creation and management of Shared Drives as part of migration workflows.

BOREAL is not intended to replace Rclone or general-purpose tools such as RcloneView. Rclone remains responsible for interacting with storage providers and transferring data. BOREAL adds an administrative and analysis layer around those capabilities, with an emphasis on inventory, metadata analysis, auditing, and migration planning.

The application is being developed around a local WebUI backed by Rust, with persistent metadata stored locally so that operations such as folder-size analysis, ownership queries, permission auditing, and migration tracking do not need to repeatedly interrogate the remote service.



## Build Dependencies

BOREAL is written in Rust and builds as a standalone binary for Linux, Windows, and macOS.

### Linux Build Environment

```
./tools/setup-build-linux.sh
```

