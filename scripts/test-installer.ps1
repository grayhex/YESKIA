# CI-only: exercise the actual NSIS installer, including its /UPDATE mode.
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true') { throw 'Run this test on an isolated GitHub Windows runner.' }
$installers = @(Get-ChildItem dist-release -File)
if ($installers.Count -ne 1 -or $installers[0].Name -notlike '*-setup.exe') {
    throw 'The build must contain exactly one installer.'
}
$installer = $installers[0].FullName
$registry = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\YESKIA'
if (Test-Path $registry) { throw 'The test requires a runner without YESKIA installed.' }
$installDir = Join-Path $env:RUNNER_TEMP 'YESKIA installer test'
$app = Join-Path $installDir 'yeskia-app.exe'
$dataDir = Join-Path $env:APPDATA 'dev.yeskia.app'
$marker = Join-Path $dataDir ('installer-test-' + [guid]::NewGuid().ToString() + '.txt')
$previousDataDir = $env:YESKIA_DATA_DIR
$version = (Get-Content src-tauri/tauri.conf.json -Raw | ConvertFrom-Json).version

function Run-Installer([string]$arguments) {
    $process = Start-Process -FilePath $installer -ArgumentList $arguments -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "Installer failed: $($process.ExitCode)" }
    if (!(Test-Path $app)) { throw 'Installed executable not found.' }
    $entry = Get-ItemProperty $registry
    if ($entry.DisplayVersion -ne $version) { throw 'Installed version mismatch.' }
    foreach ($resource in @('THIRD_PARTY_NOTICES.md', 'licenses/s60modern.txt', 'licenses/PHONE_ARTWORK.md')) {
        if (!(Test-Path (Join-Path $installDir $resource))) { throw "Missing installed license: $resource" }
    }
}
function Test-InstalledApp {
    $output = & $app --smoke-test | Out-String
    if ($LASTEXITCODE -ne 0) { throw 'Installed application smoke test failed.' }
    if (($output | ConvertFrom-Json).storage_ok -ne $true) { throw 'Installed storage check failed.' }
}
try {
    Remove-Item Env:YESKIA_DATA_DIR -ErrorAction SilentlyContinue
    # /D must be the final argument, without quotes, including when the path has spaces.
    Run-Installer "/S /D=$installDir"
    Test-InstalledApp
    Set-Content -Path $marker -Value 'keep user data' -NoNewline
    $hash = (Get-FileHash $marker).Hash
    # Same installer repairs/reinstalls the current version, using the remembered install path.
    Run-Installer '/S'
    if ((Get-FileHash $marker).Hash -ne $hash) { throw 'Reinstall changed user data.' }
    # Tauri's update mode must also reuse the installed app and preserve its data.
    Run-Installer '/S /UPDATE'
    if ((Get-FileHash $marker).Hash -ne $hash) { throw 'Update changed user data.' }
    Test-InstalledApp
    if (Test-Path (Join-Path $installDir 'yeskia.portable')) { throw 'Unexpected portable marker.' }
    Write-Output 'Installer, reinstall, update mode, installed licenses and data preservation: OK'
} finally {
    $uninstaller = Join-Path $installDir 'uninstall.exe'
    if (Test-Path $uninstaller) {
        $process = Start-Process -FilePath $uninstaller -ArgumentList '/S /UPDATE' -Wait -PassThru
        if ($process.ExitCode -ne 0) { Write-Warning "Cleanup uninstall exited $($process.ExitCode)" }
    }
    Remove-Item $marker -ErrorAction SilentlyContinue
    if ($null -ne $previousDataDir) { $env:YESKIA_DATA_DIR = $previousDataDir }
}
