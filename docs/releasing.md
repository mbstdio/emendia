# Releases

[Back to README](../README.md)

## Distributions

Each release contains Windows and Linux x64 distributions:

```text
Emendia-v0.2.0-windows-x64.exe
Emendia-v0.2.0-windows-x64-setup.exe
Emendia-v0.2.0-linux-x64.tar.gz
Emendia-v0.2.0-linux-x64.deb
SHA256SUMS.txt
```

The portable download is the executable itself, not an archive. It does not require installation, but settings still live in the Windows user profile and API keys in Windows Credential Manager. It is not a separate configuration mode.

The Inno Setup installer installs the same executable as `emendia.exe` in `%LOCALAPPDATA%\Programs\Emendia`, without administrator rights. It creates a Start Menu shortcut, offers an optional desktop shortcut and registers an uninstaller. Its interface supports English and French.

Both distributions require Windows 10 version 1903 or newer (x64) and graphics drivers compatible with GPUI. Release builds statically link the Visual C++ runtime so users do not need a separate redistributable or accompanying DLLs. The executables and installer are currently unsigned.

The Linux archive is built on Ubuntu 24.04 x64 (glibc 2.39 or newer). It includes
the executable, license, PNG icon, `.desktop` launcher and installation guide.
It requires an X11 login session and the native runtime libraries listed in the
[Linux distribution guide](../packaging/linux/README.md); it is not a static binary.

The Linux `.deb` installs the same executable in `/usr/bin`, plus a system-wide
launcher, icon, license and documentation. Library dependencies and their minimum
versions are derived from the executable with `dpkg-shlibdeps`; APT installs them
automatically. It targets Ubuntu 24.04 and compatible derivatives. Debian 13 has
not yet been validated, and Debian 12 requires a build on an older base system.

## Publish a release

The workflow is `.github/workflows/release.yml`. **Pushing a version tag creates the release automatically**; do not create or publish a release manually beforehand.

1. Update `version` in `Cargo.toml`, for example to `0.2.0`.
2. Run `cargo check` to update the root package version in `Cargo.lock`.
3. Commit the version changes along with the code to release, and push that commit to GitHub.
4. Create an annotated tag on that commit and push it:

   ```powershell
   git tag -a v0.2.0 -m "Release v0.2.0"
   git push origin v0.2.0
   ```

The tag must exactly match `v` followed by the Cargo package version. Supported versions are `MAJOR.MINOR.PATCH`, optionally followed by a SemVer prerelease suffix. Build metadata (`+...`) is not supported in release filenames.

The current package version is `0.3.0`, so use `v0.3.0`. Ensure the tag includes the workflow and packaging configuration.

### What happens automatically

1. Check out the exact tag and full Git history.
2. Install Rust 1.99.0 on the self-hosted Windows x64 runner `mb-nest-1` and reuse its persistent Cargo build directory.
3. Validate the package version against the tag.
4. Run formatting checks, Clippy and non-ignored tests.
5. Compile once with `cargo build --release --locked` and static CRT linkage.
6. Copy the executable to its portable filename and build the installer with Inno Setup 6.7.3.
7. Verify the absence of external CRT dependencies, generate SHA-256 checksums and save the distributions as Actions artifacts for 14 days.
8. Generate English release notes with git-cliff 2.14.2 for the checked-out tag.
9. Verify installation/update/uninstall on a clean GitHub-hosted `windows-2022` machine using the exact built files and the test script from the built commit.
10. In parallel, build Linux on `ubuntu-24.04`, run formatting/Clippy/tests and X11 integration/graphical diagnostics under Xvfb/Openbox, then package the `.tar.gz` archive and `.deb`. Verify Debian package installation and removal on the runner.
11. After Windows build, installer verification and Linux build succeed, create or resume a draft, combine Windows/Linux SHA-256 entries, upload all five assets, then publish.

The publication job uses GitHub's automatic token with `contents: write`. No personal access token or additional secret is needed. GitHub Actions must be enabled and repository or organization policies must allow these actions and release publication.

### Self-hosted runner setup

The build job selects the runner group `MBStudio` with the labels `[self-hosted, windows, x64]`. GitHub routes jobs by groups and labels, not by the runner's display name. The Windows x64 runner `mb-nest-1` in this group is eligible; if the group contains other matching runners, GitHub may select one of them. No custom label is required.

For the organization-level runner in `mbstdio`, open **Organization Settings → Actions → Runner groups → MBStudio** and ensure it allows the `emendia` repository. Then open **Actions → Runners → mb-nest-1** to check its Windows/x64 labels and online status. The group must also permit this repository's visibility and this workflow if workflow restrictions are enabled.

Install these prerequisites on the runner, available to the Windows account running its service:

- A recent GitHub Actions runner compatible with Node 24 actions (runner 2.327.1 or newer).
- PowerShell 7 (`pwsh`), Git for Windows, and Rustup/Cargo in `PATH`.
- Visual Studio Build Tools with Desktop development with C++, MSVC x64/x86, a recent Windows SDK, and CMake, as described in the [development guide](development.md#windows-build-environment).
- Network access to GitHub and the Rust package/toolchain downloads.

Restart the runner service after installing tools or changing its environment so that it receives the updated `PATH`. The workflow installs the pinned Rust toolchain and installs Inno Setup and git-cliff once into the runner's tool cache, reusing them on later runs. Release-note generation uses native PowerShell and the Windows git-cliff binary.

Compiled Cargo files are kept in `${{ runner.tool_cache }}/emendia/mbstdio/emendia/target`, outside the checked-out repository. This directory and the service account's Cargo registry persist between jobs and across release tags on the same runner; checkout can still clean the source tree. The GitHub Cargo-cache action is no longer needed for this persistent runner. The first build is cold, and toolchain/dependency/compiler-option changes can still require recompilation. Removing the directory or replacing the runner loses this local cache.

Installer tests run on a disposable GitHub-hosted Windows profile so that actual installs, shortcuts and startup-registry checks do not affect the self-hosted machine's user profile. Compilation and packaging run on `mb-nest-1`; installer verification and publication use hosted runners.

### Prereleases

Set the Cargo package version to, for example, `0.3.0-beta.1`, update `Cargo.lock`, commit and push, then:

```powershell
git tag -a v0.3.0-beta.1 -m "Release v0.3.0-beta.1"
git push origin v0.3.0-beta.1
```

Any version with a prerelease suffix, including `beta` or `rc`, is published as a GitHub prerelease and does not become the latest stable release.

## Trigger a build without publishing

Once the workflow is on the default branch:

1. Open **Actions → Build and release → Run workflow**.
2. Select a branch in GitHub's workflow selector.
3. Optionally enter a branch, tag or commit in **ref** to build that revision instead.
4. Run the workflow and download its **Artifacts** after it succeeds.

Manual builds run the same checks and produce Windows and Linux distributions and checksums, but never publish a release, even when building a tag. Their filenames contain the package version, `dev` and the short commit ID, for example `Emendia-v0.2.0-dev-a1b2c3d4-windows-x64.exe`. Linux artifacts include `SHA256SUMS-linux.txt`; published releases combine its entries into the shared `SHA256SUMS.txt`.

GitHub packages Actions artifacts in a ZIP for download. This is only the test-build transport; the portable asset on a published release is a direct `.exe` download.

## Release notes

`cliff.toml` groups commits into **Breaking Changes**, **Features**, **Bug Fixes**, **Performance**, **Maintenance** and **Other Changes**. Write commit subjects in English, preferably using Conventional Commits:

```text
feat: add a new proofreading style
fix: restore focus after replacement
docs: clarify provider configuration
feat!: change the configuration format
```

Breaking changes are highlighted. Unconventional commits are retained in the fallback category. Notes include the commits since the preceding reachable version tag, including prerelease tags. For the first release, all commits up to the tag are included; later commits are excluded. Notes organize commit descriptions without translating or rewriting them.

The preceding tag is resolved with `git describe` on the checked-out commit's ancestry, excluding the current tag and non-version tags. Its commit and the current commit define an explicit range for git-cliff, avoiding alphabetical tag ordering around beta-to-stable and numeric version transitions.

The workflow owns the draft title and description and regenerates them when resuming publication. Add any optional manual introduction after publication. Published releases are not overwritten by reruns.

## Recover a failed build or publication

- Open the failed run and select **Re-run failed jobs** or **Re-run all jobs** after fixing any external issue.
- If publication stopped after creating a draft, rerunning resumes it, refreshes its notes and replaces partially uploaded assets before publishing.
- If the release is already published, publication skips it without replacing its files.
- If a code or workflow change is required, commit the fix and use a new version/tag. Do not move a released tag.

Builds for the same trigger and ref are serialized to avoid simultaneous publication. Build artifacts remain available even if the publication job fails. If they have expired, rerun all jobs to regenerate them.

## Installer behavior

Updates use the same installation identity and directory. Setup and uninstall refuse to proceed while the installed executable is locked or not writable; choose **Quit** from Emendia's tray menu, then retry. Automatic startup remains controlled by Emendia's settings, rather than a second installer option. Settings and API keys are preserved during upgrades and uninstall.

Uninstall removes the `Emendia` startup registry entry only when it points to the executable in the installation being removed. A startup entry belonging to a portable copy is preserved. When switching from a portable copy to the installed version, save the startup setting from the installed app to update its path.

## Local packaging and validation

The Windows executable, native window/taskbar resources, installer and uninstaller use `src/ressources/app-logo.ico`, with sizes from 16 to 256 pixels. When changing that logo, regenerate the ICO before building; the build script embeds it as Windows resource 1. Shortcuts and the installed-apps entry use the executable's icon.

The tray embeds `src/ressources/app-logo-light.png` and `app-logo-dark.png` for
the matching system themes on Windows and Linux. Linux window/taskbar icons
always use the light PNG as the base logo. The Linux archive installs that same
asset as `emendia.png` for the `.desktop` launcher; the executable also publishes
it through X11 without requiring launcher installation.

Install Inno Setup 6.7.3 and make `ISCC.exe` available in `PATH`. From PowerShell at the repository root:

```powershell
$env:RUSTFLAGS = '-C target-feature=+crt-static'
cargo build --release --locked
ISCC /DAppVersion=0.3.0 /DBuildLabel=v0.3.0 packaging/windows/emendia.iss
```

Use the actual package version. The installer is written to `target/dist`. Quit any running copy of `target/release/emendia.exe` before rebuilding it.

With git-cliff installed, preview the next release's notes before tagging:

```powershell
git-cliff --unreleased --tag v0.2.0
```

Before the first public release, use a manual build and validate on a clean Windows machine:

- Run the portable EXE without development tools or a Visual C++ redistributable installed.
- Install without administrator rights and check English/French setup and shortcuts.
- Upgrade an existing installation, including while Emendia is running.
- Verify settings and credentials remain available.
- Enable startup, uninstall, and confirm the owned startup entry is removed.
- Confirm uninstall preserves a startup entry pointing to a portable copy.
- Check the downloaded files with `Get-FileHash -Algorithm SHA256` against `SHA256SUMS.txt`.

Interactive desktop/clipboard tests from the [development guide](development.md#verification) remain manual checks; they are not enabled on hosted CI runners.

`packaging/windows/test-installer.ps1` automates binary identity, repeated installation, locked-file guards and startup cleanup checks in CI. It installs into a temporary directory and refuses to run if the user profile already has an Emendia installation, Start Menu shortcut or startup entry. Run it only on a disposable profile.

### Local Linux packaging

```sh
cargo build --release --locked
EMENDIA_BINARY=target/release/emendia bash packaging/linux/test-x11.sh
bash packaging/linux/package.sh v0.2.0
sha256sum target/dist/Emendia-v0.2.0-linux-x64.tar.gz
sha256sum target/dist/Emendia-v0.2.0-linux-x64.deb
```

`desktop-file-utils` is required to validate the launcher, and `dpkg-dev` is required
to derive dependencies and build the Debian package. Both distributions are written
to `target/dist`, with checksums in `SHA256SUMS-linux.txt`. Prerelease and manual
build versions use Debian's `~` separator so they sort before stable releases.
Test them on a clean Ubuntu 24.04 X11 desktop, especially tray
integration, keyring persistence and autostart, before publishing the first Linux
release. The workflow checks linked dependencies with `ldd` and waits for all
platform verification jobs before publication.
