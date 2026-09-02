param(
  [Parameter(Mandatory = $true)]
  [string] $InstallRoot
)

$ErrorActionPreference = "Stop"

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

function Stop-ExactProcesses {
  param(
    [Parameter(Mandatory = $true)]
    [string] $ExecutablePath,
    [Parameter(Mandatory = $true)]
    [string] $ExecutableName
  )

  foreach ($record in @(Get-ExactProcesses $ExecutablePath $ExecutableName)) {
    Stop-Process -Id $record.ProcessId -Force -ErrorAction SilentlyContinue
  }
}

$root = [IO.Path]::GetFullPath($InstallRoot)
$desktopPath = Join-Path $root "openaide-desktop.exe"
$serverPath = Join-Path $root "openaide-app-server.exe"

# Let current and future Desktop builds cross their graceful client-detach
# boundary before the installer considers force. Older builds may close first
# and let the App Server drain through its reconnect grace instead.
foreach ($record in @(Get-ExactProcesses $desktopPath "openaide-desktop.exe")) {
  $desktop = Get-Process -Id $record.ProcessId -ErrorAction SilentlyContinue
  if ($desktop) {
    [void] $desktop.CloseMainWindow()
  }
}
Start-Sleep -Seconds 5
Stop-ExactProcesses $desktopPath "openaide-desktop.exe"

# A server left by an older/interrupted run has no Desktop window for Tauri's
# built-in process check. Release only the executable belonging to this install.
Stop-ExactProcesses $serverPath "openaide-app-server.exe"
Start-Sleep -Seconds 1

if (@(Get-ExactProcesses $desktopPath "openaide-desktop.exe").Count -gt 0) {
  throw "The installed OpenAIDE Desktop process could not be stopped."
}
if (@(Get-ExactProcesses $serverPath "openaide-app-server.exe").Count -gt 0) {
  throw "The installed OpenAIDE App Server process could not be stopped."
}
