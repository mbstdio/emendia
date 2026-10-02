# Run on a disposable Windows user profile (as in GitHub-hosted CI).
param(
    [Parameter(Mandatory)][string]$Installer,
    [Parameter(Mandatory)][string]$PortableExe
)

$ErrorActionPreference = 'Stop'
$Installer = (Resolve-Path $Installer).Path
$PortableExe = (Resolve-Path $PortableExe).Path
$uninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{5BFA5FAF-5F24-4D2B-9D6B-A8DA0D05C353}_is1'
$runKey = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Software\Microsoft\Windows\CurrentVersion\Run')
$shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'Emendia.lnk'
if ((Test-Path $uninstallKey) -or (Test-Path $shortcut) -or ($null -ne $runKey.GetValue('Emendia'))) {
    $runKey.Dispose()
    throw 'Installer tests require a profile without an existing Emendia installation, shortcut or startup entry'
}

$sandbox = Join-Path ([IO.Path]::GetTempPath()) "emendia-installer-test-$([Guid]::NewGuid())"
$installedExe = Join-Path $sandbox 'emendia.exe'
$uninstaller = Join-Path $sandbox 'unins000.exe'
$arguments = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/LANG=english', "/DIR=`"$sandbox`"")
$expectedHash = (Get-FileHash $PortableExe -Algorithm SHA256).Hash

function Install-App {
    $process = Start-Process $Installer -ArgumentList $arguments -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "Installation failed: $($process.ExitCode)" }
    if ((Get-FileHash $installedExe -Algorithm SHA256).Hash -ne $expectedHash) {
        throw 'Installed executable differs from the portable executable'
    }
    if (-not (Test-Path $uninstallKey) -or -not (Test-Path $shortcut)) {
        throw 'Uninstall registration or Start Menu shortcut is missing'
    }
}

function Uninstall-App {
    $process = Start-Process $uninstaller -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART' -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "Uninstall failed: $($process.ExitCode)" }
    if ((Test-Path $installedExe) -or (Test-Path $uninstallKey) -or (Test-Path $shortcut)) {
        throw 'Uninstall did not remove the executable, registration and shortcut'
    }
}

try {
    Install-App
    Install-App # Same identity and executable on a repeated installation.

    # Exercise the update/uninstall guards without requiring a graphical desktop.
    $lock = [IO.File]::Open($installedExe, 'Open', 'Read', 'None')
    try {
        $update = Start-Process $Installer -ArgumentList $arguments -Wait -PassThru
        if ($update.ExitCode -eq 0) { throw 'Setup accepted a locked executable' }
        $remove = Start-Process $uninstaller -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART' -Wait -PassThru
        if ($remove.ExitCode -eq 0) { throw 'Uninstall accepted a locked executable' }
    } finally {
        $lock.Dispose()
    }
    if ((Get-FileHash $installedExe -Algorithm SHA256).Hash -ne $expectedHash) {
        throw 'A blocked upgrade altered the executable'
    }

    $runKey.SetValue('Emendia', "`"$installedExe`"")
    Uninstall-App
    if ($null -ne $runKey.GetValue('Emendia')) { throw 'Owned startup entry was not removed' }

    Install-App
    $portableCommand = "`"$PortableExe`""
    $runKey.SetValue('Emendia', $portableCommand)
    Uninstall-App
    if ($runKey.GetValue('Emendia') -cne $portableCommand) {
        throw 'Uninstall changed the portable copy startup entry'
    }
    Write-Output 'Installer checks passed: binary identity, repeated install, lock guards and startup cleanup'
} finally {
    if (Test-Path $uninstaller) {
        Start-Process $uninstaller -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART' -Wait | Out-Null
    }
    $runKey.DeleteValue('Emendia', $false)
    $runKey.Dispose()
    if (Test-Path $sandbox) { Remove-Item $sandbox -Recurse -Force }
}
