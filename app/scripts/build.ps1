$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$npmCommand = Get-Command npm.cmd -ErrorAction SilentlyContinue
$portableNpm = Join-Path $projectRoot '.dev\npm\package\bin\npm-cli.js'
function Invoke-FrontendNpm([string[]]$NpmArguments) {
    if ($npmCommand) { & $npmCommand.Source @NpmArguments }
    elseif (Test-Path -LiteralPath $portableNpm) { & node $portableNpm @NpmArguments }
    else { throw 'Install Node.js 22.13 or newer with npm before building the React frontend.' }
    if ($LASTEXITCODE -ne 0) { throw "Frontend build failed ($LASTEXITCODE)" }
}
Push-Location (Join-Path $projectRoot 'frontend')
try {
    Invoke-FrontendNpm -NpmArguments @('ci','--no-audit','--no-fund')
    Invoke-FrontendNpm -NpmArguments @('run','build')
} finally { Pop-Location }
& "$PSScriptRoot\cargo.ps1" build --release --locked
New-Item -ItemType Directory -Force -Path (Join-Path $projectRoot 'app\data'),(Join-Path $projectRoot 'app\logs') | Out-Null
Copy-Item -LiteralPath (Join-Path $projectRoot 'target\release\paddock.exe') -Destination (Join-Path $projectRoot 'app\launcher.exe') -Force
Copy-Item -LiteralPath (Join-Path $projectRoot 'target\release\WebView2Loader.dll') -Destination (Join-Path $projectRoot 'app\WebView2Loader.dll') -Force
Write-Output "Built: $projectRoot\app\launcher.exe (data preserved)"
