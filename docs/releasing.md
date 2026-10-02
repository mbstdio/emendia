# Releases

[Back to README](../README.md)

## Distributions

Each Windows x64 release contains three assets:

```text
Emendia-v0.2.0-windows-x64.exe
Emendia-v0.2.0-windows-x64-setup.exe
SHA256SUMS.txt
```

The portable download is the executable itself, not an archive. It does not require installation, but settings still live in the Windows user profile and API keys in Windows Credential Manager. It is not a separate configuration mode.

The Inno Setup installer installs the same executable as `emendia.exe` in `%LOCALAPPDATA%\Programs\Emendia`, without administrator rights. It creates a Start Menu shortcut, offers an optional desktop shortcut and registers an uninstaller. Its interface supports English and French.

Both distributions require Windows 10 version 1903 or newer (x64) and graphics drivers compatible with GPUI. Release builds statically link the Visual C++ runtime so users do not need a separate redistributable or accompanying DLLs. The executables and installer are currently unsigned.

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

For the first release, the current package version is `0.1.0`, so use `v0.1.0`. Ensure the tag includes the workflow and packaging configuration.

### What happens automatically

1. Check out the exact tag and full Git history.
2. Install Rust 1.99.0 on `windows-2022` and restore the Cargo cache.
3. Validate the package version against the tag.
4. Run formatting checks, Clippy and non-ignored tests.
5. Compile once with `cargo build --release --locked` and static CRT linkage.
6. Copy the executable to its portable filename and build the installer with Inno Setup 6.7.3.
7. Verify the absence of external CRT dependencies, check installation/update/uninstall behavior, generate SHA-256 checksums and save the distributions as Actions artifacts for 14 days.
8. Generate English release notes with git-cliff 2.14.2 for the checked-out tag.
9. In a separate publication job, create or resume a draft, upload all three assets, then publish.

The publication job uses GitHub's automatic token with `contents: write`. No personal access token or additional secret is needed. GitHub Actions must be enabled and repository or organization policies must allow these actions and release publication.

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

Manual builds run the same checks and produce both distributions and checksums, but never publish a release, even when building a tag. Their filenames contain the package version, `dev` and the short commit ID, for example `Emendia-v0.2.0-dev-a1b2c3d4-windows-x64.exe`.

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

Install Inno Setup 6.7.3 and make `ISCC.exe` available in `PATH`. From PowerShell at the repository root:

```powershell
$env:RUSTFLAGS = '-C target-feature=+crt-static'
cargo build --release --locked
ISCC /DAppVersion=0.1.0 /DBuildLabel=v0.1.0 packaging/windows/emendia.iss
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
