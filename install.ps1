# speedtest-rli installer for Windows.
# Usage: irm https://github.com/andreasmlbngaol/speedtest-rli/releases/latest/download/install.ps1 | iex
$ErrorActionPreference = 'Stop'

$repo = 'andreasmlbngaol/speedtest-rli'
$url  = "https://github.com/$repo/releases/latest/download/speedtest-rli-windows-x86_64.exe"

$dir = Join-Path $env:LOCALAPPDATA 'speedtest-rli'
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$exe = Join-Path $dir 'speedtest-rli.exe'

Write-Host "Downloading speedtest-rli..."
Invoke-WebRequest -Uri $url -OutFile $exe

# Add the install folder to the *user* PATH (no admin rights needed).
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($null -eq $userPath) { $userPath = '' }
if ($userPath -notlike "*$dir*") {
    $newPath = if ([string]::IsNullOrEmpty($userPath)) { $dir } else { "$userPath;$dir" }
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    Write-Host "Added $dir to your PATH."
}

Write-Host ""
Write-Host "Installed: $exe"
Write-Host "Open a NEW terminal window (PowerShell or Command Prompt) and run: speedtest-rli"
