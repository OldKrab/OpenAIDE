param(
  [Parameter(Mandatory = $true)]
  [string] $Installer
)

$ErrorActionPreference = "Stop"
$installRoot = Join-Path $env:RUNNER_TEMP "openaide-desktop-lifecycle"
$decoyRoot = Join-Path $env:RUNNER_TEMP "openaide-app-server-decoy"
$desktop = $null
$decoy = $null

Add-Type @"
using System;
using System.Runtime.InteropServices;

public static class OpenAideWindowMessages {
    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool PostMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);
}
"@

function Wait-ForCondition {
  param(
    [Parameter(Mandatory = $true)]
    [scriptblock] $Condition,
    [Parameter(Mandatory = $true)]
    [int] $TimeoutSeconds,
    [Parameter(Mandatory = $true)]
    [string] $Failure
  )

  $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
  do {
    if (& $Condition) { return }
    Start-Sleep -Milliseconds 200
  } while ([DateTime]::UtcNow -lt $deadline)
  throw $Failure
}

function Get-ProcessAtPath {
  param([Parameter(Mandatory = $true)][string] $ExecutablePath)

  $expected = [IO.Path]::GetFullPath($ExecutablePath)
  return @(
    Get-CimInstance Win32_Process -Filter "Name = 'openaide-app-server.exe'" |
      Where-Object {
        $_.ExecutablePath -and [IO.Path]::GetFullPath($_.ExecutablePath) -ieq $expected
      }
  )
}

function Start-Installer {
  $process = Start-Process `
    -FilePath $Installer `
    -ArgumentList @("/S", "/D=$installRoot") `
    -PassThru
  if (-not $process.WaitForExit(60000)) {
    Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    throw "Windows installer did not finish while replacing a running OpenAIDE installation"
  }
  if ($process.ExitCode -ne 0) {
    throw "Windows installer exited with code $($process.ExitCode)"
  }
}

function Start-InstalledDesktop {
  $desktopPath = Join-Path $installRoot "openaide-desktop.exe"
  $serverPath = Join-Path $installRoot "openaide-app-server.exe"
  $process = Start-Process -FilePath $desktopPath -PassThru
  Wait-ForCondition `
    -TimeoutSeconds 20 `
    -Failure "Installed Desktop did not create its main window" `
    -Condition {
      if ($process.HasExited) { throw "Installed Desktop exited during startup" }
      $process.Refresh()
      $process.MainWindowHandle -ne [IntPtr]::Zero
    }
  Wait-ForCondition `
    -TimeoutSeconds 20 `
    -Failure "Installed Desktop did not start its App Server" `
    -Condition { (Get-ProcessAtPath $serverPath).Count -eq 1 }
  return $process
}

function Start-DecoyAppServer {
  $source = Join-Path $installRoot "openaide-app-server.exe"
  $destination = Join-Path $decoyRoot "openaide-app-server.exe"
  New-Item -ItemType Directory -Force -Path $decoyRoot | Out-Null
  Copy-Item $source $destination

  $start = [Diagnostics.ProcessStartInfo]::new()
  $start.FileName = $destination
  $start.UseShellExecute = $false
  $start.CreateNoWindow = $true
  $start.RedirectStandardOutput = $true
  $start.Environment["OPENAIDE_APP_SERVER_PROTOCOL"] = "app-server-handoff"
  $start.Environment["OPENAIDE_STORAGE_ROOT"] = Join-Path $decoyRoot "state"
  $start.Environment["OPENAIDE_RUNTIME_ROOT"] = Join-Path $decoyRoot "runtime"
  $process = [Diagnostics.Process]::Start($start)
  $line = $process.StandardOutput.ReadLineAsync()
  if (-not $line.Wait(10000) -or [string]::IsNullOrWhiteSpace($line.Result)) {
    $process.Kill()
    throw "Decoy App Server did not publish its endpoint"
  }
  return $process
}

try {
  if (Test-Path $installRoot) { Remove-Item -Recurse -Force $installRoot }
  if (Test-Path $decoyRoot) { Remove-Item -Recurse -Force $decoyRoot }

  Start-Installer

  # A real native close request covers Alt+F4 and the Windows caption button.
  $desktop = Start-InstalledDesktop
  # The native handle and sidecar exist before the frontend installs its command
  # listener. Exercise user close only after that startup boundary has settled.
  Start-Sleep -Seconds 2
  if (-not [OpenAideWindowMessages]::PostMessage(
    $desktop.MainWindowHandle,
    0x0010,
    [IntPtr]::Zero,
    [IntPtr]::Zero
  )) {
    throw "Could not send the native Desktop close request"
  }
  if (-not $desktop.WaitForExit(10000)) {
    throw "Desktop did not exit after its native close request"
  }
  $installedServer = Join-Path $installRoot "openaide-app-server.exe"
  Wait-ForCondition `
    -TimeoutSeconds 5 `
    -Failure "Desktop native close left its installed App Server running" `
    -Condition { (Get-ProcessAtPath $installedServer).Count -eq 0 }
  $desktop = $null

  # Reinstall must release only the binary inside this installation. A process
  # with the same name elsewhere proves the fallback is path-scoped.
  $desktop = Start-InstalledDesktop
  Start-Sleep -Seconds 2
  $decoy = Start-DecoyAppServer
  Start-Installer
  if ($decoy.HasExited) {
    throw "Installer terminated an App Server outside its installation directory"
  }

  # Releases before the native-close fix could exit Desktop without detaching,
  # leaving the installed App Server locked during the next install. Recreate
  # that upgrade boundary and require the installer fallback to release it.
  $desktop = Start-InstalledDesktop
  Start-Sleep -Seconds 2
  Stop-Process -Id $desktop.Id -Force
  if (-not $desktop.WaitForExit(5000)) {
    throw "Could not recreate the previous Desktop shutdown behavior"
  }
  Wait-ForCondition `
    -TimeoutSeconds 2 `
    -Failure "Previous Desktop shutdown behavior did not leave its App Server running" `
    -Condition { (Get-ProcessAtPath $installedServer).Count -eq 1 }
  $desktop = $null
  Start-Installer
  if ($decoy.HasExited) {
    throw "Installer fallback terminated an App Server outside its installation directory"
  }
  Write-Host "Windows Desktop close and reinstall lifecycle smoke passed"
} finally {
  if ($desktop -and -not $desktop.HasExited) {
    Stop-Process -Id $desktop.Id -Force -ErrorAction SilentlyContinue
  }
  if ($decoy -and -not $decoy.HasExited) {
    Stop-Process -Id $decoy.Id -Force -ErrorAction SilentlyContinue
  }
  foreach ($process in Get-ProcessAtPath (Join-Path $installRoot "openaide-app-server.exe")) {
    Stop-Process -Id $process.ProcessId -Force -ErrorAction SilentlyContinue
  }
  if (Test-Path $installRoot) { Remove-Item -Recurse -Force $installRoot }
  if (Test-Path $decoyRoot) { Remove-Item -Recurse -Force $decoyRoot }
}
