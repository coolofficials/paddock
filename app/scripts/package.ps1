param([string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $projectRoot 'outputs' }
$version = [regex]::Match((Get-Content -LiteralPath (Join-Path $projectRoot 'Cargo.toml') -Raw),'(?m)^version = "([^"]+)"').Groups[1].Value
if (-not $version) { throw 'Package version is missing.' }
$stage = Join-Path $projectRoot ('work\release-' + [Guid]::NewGuid().ToString('N'))
$package = Join-Path $stage ('Paddock-' + $version)
New-Item -ItemType Directory -Force -Path $package,(Join-Path $package 'licenses'),$OutputDirectory | Out-Null
Copy-Item -LiteralPath (Join-Path $projectRoot 'LICENSE') -Destination $package
foreach ($filename in @('paddock.exe','WebView2Loader.dll')) {
    Copy-Item -LiteralPath (Join-Path $projectRoot ('target\release\'+$filename)) -Destination $package
}
Copy-Item -LiteralPath (Join-Path $projectRoot ('docs\RELEASE-'+$version+'.txt')) -Destination (Join-Path $package 'README.txt')
Copy-Item -LiteralPath (Join-Path $projectRoot ('docs\REACT-UI-'+$version+'.md')) -Destination (Join-Path $package 'ARCHITECTURE.md')
Copy-Item -LiteralPath (Join-Path $projectRoot 'docs\THIRD_PARTY_NOTICES.txt') -Destination $package
Copy-Item -LiteralPath (Join-Path $projectRoot ('docs\DEPENDENCY-NOTICES-'+$version+'.txt')) -Destination (Join-Path $package 'licenses\DEPENDENCY-NOTICES.txt')
Get-ChildItem -LiteralPath (Join-Path $projectRoot 'assets\framework\licenses') -File | ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $package 'licenses') }
Copy-Item -LiteralPath (Join-Path $projectRoot 'assets\webview2\LICENSE.txt') -Destination (Join-Path $package 'licenses\Microsoft-WebView2-LICENSE.txt')
Copy-Item -LiteralPath (Join-Path $projectRoot 'assets\webview2\NOTICE.txt') -Destination (Join-Path $package 'licenses\Microsoft-WebView2-NOTICE.txt')
$archive = Join-Path $OutputDirectory ('Paddock-'+$version+'-Windows-x64.zip')
Compress-Archive -LiteralPath $package -DestinationPath $archive -Force
Write-Output "Packaged: $archive"
