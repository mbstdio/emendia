# Run on a dedicated, unlocked Windows desktop; tests temporarily use focus and the clipboard.
param(
    [string]$Binary = '',
    [ValidateRange(10, 300)][int]$TimeoutSeconds = 45
)

$ErrorActionPreference = 'Stop'
if (-not [Environment]::UserInteractive -or [Diagnostics.Process]::GetCurrentProcess().SessionId -eq 0) {
    throw 'Desktop diagnostics require an interactive runner started in an unlocked user session, not a Windows service'
}

if (-not $Binary) {
    if ($env:EMENDIA_BINARY) {
        $Binary = $env:EMENDIA_BINARY
    } else {
        $target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { 'target' }
        $Binary = Join-Path $target 'debug/emendia.exe'
    }
}
$Binary = (Resolve-Path -LiteralPath $Binary).Path

function Invoke-SmokeDiagnostic {
    param([string[]]$Arguments)
    $process = Start-Process -FilePath $Binary -ArgumentList $Arguments -PassThru
    try {
        if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
            $process.Kill()
            $process.WaitForExit()
            throw "Desktop diagnostic timed out: $($Arguments -join ' ')"
        }
        $process.Refresh()
        if ($process.ExitCode -ne 0) {
            throw "Desktop diagnostic failed ($($process.ExitCode)): $($Arguments -join ' ')"
        }
    } finally {
        $process.Dispose()
    }
}

$temporary = [IO.Path]::GetTempPath()
if (-not (Test-Path -LiteralPath $temporary -PathType Container)) {
    throw 'The temporary directory is unavailable'
}
$sandbox = Join-Path $temporary "emendia-desktop-test-$([Guid]::NewGuid())"
New-Item -ItemType Directory -Path $sandbox | Out-Null
$previousProfile = $env:EMENDIA_SMOKE_CONFIG_DIR
$env:EMENDIA_SMOKE_CONFIG_DIR = $sandbox
try {
    $cargoArguments = @()
    if ($env:RUST_TOOLCHAIN) { $cargoArguments += "+$env:RUST_TOOLCHAIN" }
    $cargoArguments += @(
        'test', '--locked', '--lib',
        'platform::windows::desktop_tests::clipboard_and_native_edit_round_trip',
        '--', '--exact', '--ignored', '--nocapture', '--test-threads=1'
    )
    & cargo @cargoArguments
    if ($LASTEXITCODE -ne 0) { throw 'Native Windows capture, replacement and clipboard checks failed' }

    foreach ($language in 'en', 'fr') {
        foreach ($mode in '--smoke-test-onboarding', '--smoke-test', '--smoke-test-quick', '--smoke-test-correction', '--smoke-test-quick-check') {
            Invoke-SmokeDiagnostic -Arguments @($mode, "--ui-language=$language")
        }
        Invoke-SmokeDiagnostic -Arguments @('--smoke-test-onboarding', "--ui-language=$language", '--smoke-theme=dark')
    }
    Write-Output 'Windows desktop checks passed: native editor, clipboard, onboarding, previews and quick-flow recovery in English and French'
} finally {
    $env:EMENDIA_SMOKE_CONFIG_DIR = $previousProfile
    Remove-Item -LiteralPath $sandbox -Recurse -Force
}
