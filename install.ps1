# speedtest installer for Windows.
# Usage: irm https://github.com/andreasmlbngaol/speedtest-rli/releases/latest/download/install.ps1 | iex
$ErrorActionPreference = 'Stop'

$repo = 'andreasmlbngaol/speedtest-rli'
$base = "https://github.com/$repo/releases/latest/download"
$url  = "$base/speedtest-rli-windows-x86_64.exe"
$trayUrl = "$base/speedtest-rli-tray-windows-x86_64.exe"

$dir = Join-Path $env:LOCALAPPDATA 'speedtest-rli'
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$exe = Join-Path $dir 'speedtest.exe'

Write-Host "Downloading speedtest..."
Invoke-WebRequest -Uri $url -OutFile $exe
# Keep the old name working too, in case scripts call speedtest-rli.
Copy-Item -Path $exe -Destination (Join-Path $dir 'speedtest-rli.exe') -Force

# Optional system-tray launcher (skipped if the download fails).
$trayExe = Join-Path $dir 'speedtest-tray.exe'
try {
    Write-Host "Downloading speedtest tray..."
    Invoke-WebRequest -Uri $trayUrl -OutFile $trayExe
    # Autostart the tray at login via the per-user Run key (no admin rights).
    $runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
    Set-ItemProperty -Path $runKey -Name 'speedtest-tray' -Value "`"$trayExe`""
    Write-Host "Tray installed and set to start at login: $trayExe"
} catch {
    Write-Host "Note: tray binary not available; CLI installed on its own."
}

# Add the install folder to the *user* PATH (no admin rights needed).
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($null -eq $userPath) { $userPath = '' }
if ($userPath -notlike "*$dir*") {
    $newPath = if ([string]::IsNullOrEmpty($userPath)) { $dir } else { "$userPath;$dir" }
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    Write-Host "Added $dir to your PATH."
}

# Also update THIS session's PATH so it works without opening a new window.
if ($env:Path -notlike "*$dir*") {
    $env:Path = "$env:Path;$dir"
}

Write-Host ""
Write-Host "Installed: $exe"
Write-Host "You can run it right now in this window: speedtest"
Write-Host "(It also works as 'speedtest-rli'.)"
