# Running BOREAL on macOS

BOREAL supports both Apple Silicon and Intel Macs. The current beta binaries
are not code-signed or notarized, so macOS may block the first launch even when
the binary was downloaded from the official BOREAL release.

BOREAL does not require administrator privileges for normal operation. Do not
run BOREAL with `sudo`.

## 1. Choose the Correct Download

Choose **About This Mac** from the Apple menu and check the listed processor or
chip:

- Apple M1, M2, M3, or later: download `boreal-v0.1.2-macos-aarch64`.
- Intel processor: download `boreal-v0.1.2-macos-x86_64`.

The Rust target name for Apple Silicon is `aarch64-apple-darwin`. Apple also
uses the name `arm64` for this architecture.

## 2. Make BOREAL Executable

Open Terminal and run:

```bash
cd ~/Downloads
chmod +x boreal-v0.1.2-macos-aarch64
```

On an Intel Mac, substitute `boreal-v0.1.2-macos-x86_64`.

## 3. Remove the Download Quarantine Attribute

Because the beta binary is not notarized, remove the quarantine attribute from
the downloaded file:

```bash
xattr -d com.apple.quarantine boreal-v0.1.2-macos-aarch64
```

This command should not request administrator privileges when the downloaded
file belongs to your account. Do not add `sudo`.

If Terminal reports `No such xattr`, the quarantine attribute is already
absent and you can continue.

## 4. Start BOREAL

From the same Terminal window, run:

```bash
./boreal-v0.1.2-macos-aarch64
```

Keep the Terminal window open while using BOREAL. BOREAL creates its private
runtime directory at `~/.boreal`, installs its managed Rclone binary there, and
opens the local WebUI in the default browser.

## Gatekeeper Approval

If macOS still blocks the binary, open **System Settings → Privacy & Security**.
Find the message that BOREAL was blocked and select **Open Anyway**. macOS may
require an administrator account to authorize this one-time Gatekeeper
exception. That approval does not cause BOREAL to run as an administrator.

On an organization-managed Mac, security policy may prevent users from making
this exception. Contact the organization's Mac administrator rather than
disabling macOS security controls.

## Verify the Download

The release includes `SHA256SUMS`. Calculate the downloaded binary's checksum
with:

```bash
shasum -a 256 boreal-v0.1.2-macos-aarch64
```

Confirm that the result matches the corresponding entry in `SHA256SUMS` before
approving an unsigned binary.

## Troubleshooting

Confirm the binary architecture with:

```bash
file boreal-v0.1.2-macos-aarch64
```

An Apple Silicon binary should be reported as `arm64`. If BOREAL starts but
reports a runtime error, copy the complete Terminal output when requesting
support. Do not retry it with `sudo`, because that could create root-owned files
inside `~/.boreal`.
