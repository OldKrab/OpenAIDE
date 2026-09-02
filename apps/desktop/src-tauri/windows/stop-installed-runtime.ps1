param(
  [Parameter(Mandatory = $true)]
  [string] $InstallRoot
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version 3.0

function Get-ExactProcesses {
  param(
    [Parameter(Mandatory = $true)]
    [string] $ExecutablePath,
    [Parameter(Mandatory = $true)]
    [string] $ExecutableName
  )

  $expected = [IO.Path]::GetFullPath($ExecutablePath)
  return @(
    Get-CimInstance Win32_Process -Filter "Name = '$ExecutableName'" |
      Where-Object {
        $_.ExecutablePath -and [IO.Path]::GetFullPath($_.ExecutablePath) -ieq $expected
      }
  )
}

function Wait-ForExactProcessExit {
  param(
    [Parameter(Mandatory = $true)]
    [string] $ExecutablePath,
    [Parameter(Mandatory = $true)]
    [string] $ExecutableName,
    [Parameter(Mandatory = $true)]
    [int] $TimeoutSeconds
  )

  $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
  do {
    if ((Get-ExactProcesses $ExecutablePath $ExecutableName).Count -eq 0) {
      return $true
    }
    Start-Sleep -Milliseconds 200
  } while ([DateTime]::UtcNow -lt $deadline)
  return $false
}

function Stop-ExactProcesses {
  param(
    [Parameter(Mandatory = $true)]
    [string] $ExecutablePath,
    [Parameter(Mandatory = $true)]
    [string] $ExecutableName
  )

  foreach ($record in Get-ExactProcesses $ExecutablePath $ExecutableName) {
    Stop-Process -Id $record.ProcessId -Force -ErrorAction SilentlyContinue
  }
}

$root = [IO.Path]::GetFullPath($InstallRoot)
$desktopPath = Join-Path $root "openaide-desktop.exe"
$serverPath = Join-Path $root "openaide-app-server.exe"

# Let current and future Desktop builds cross their graceful client-detach
# boundary before the installer considers force. Older builds may close first
# and let the App Server drain through its reconnect grace instead.
foreach ($record in Get-ExactProcesses $desktopPath "openaide-desktop.exe") {
  $desktop = Get-Process -Id $record.ProcessId -ErrorAction SilentlyContinue
  if ($desktop) {
    [void] $desktop.CloseMainWindow()
  }
}
if (-not (Wait-ForExactProcessExit $desktopPath "openaide-desktop.exe" 10)) {
  Stop-ExactProcesses $desktopPath "openaide-desktop.exe"
}
if (-not (Wait-ForExactProcessExit $desktopPath "openaide-desktop.exe" 5)) {
  throw "The installed OpenAIDE Desktop process could not be stopped."
}

# A server left by an older/interrupted run has no Desktop window for Tauri's
# built-in process check. Wait through its normal reconnect grace, then release
# only the executable belonging to this installation.
if (-not (Wait-ForExactProcessExit $serverPath "openaide-app-server.exe" 15)) {
  Stop-ExactProcesses $serverPath "openaide-app-server.exe"
}
if (-not (Wait-ForExactProcessExit $serverPath "openaide-app-server.exe" 5)) {
  throw "The installed OpenAIDE App Server process could not be stopped."
}
