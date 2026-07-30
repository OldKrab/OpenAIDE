$ErrorActionPreference = "Stop"

$prototypeRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = (Resolve-Path (Join-Path $prototypeRoot "../../..")).Path

Push-Location $repoRoot
try {
    cargo build -p openaide-app-server
}
finally {
    Pop-Location
}

Push-Location $prototypeRoot
try {
    if (-not (Test-Path "node_modules")) {
        npm install
    }
    $env:OPENAIDE_APP_SERVER_BIN = Join-Path $repoRoot "target/debug/openaide-app-server.exe"
    $env:OPENAIDE_DESKTOP_PROTOTYPE_ROOT = $prototypeRoot
    npm run tauri -- dev
}
finally {
    Pop-Location
}
