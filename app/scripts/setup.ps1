$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$temporary = Join-Path $projectRoot '.dev\temp'
$rustRoot = Join-Path $projectRoot '.dev\rust'
$nativeRoot = Join-Path $projectRoot '.dev\native'
New-Item -ItemType Directory -Force -Path $temporary,$rustRoot,$nativeRoot | Out-Null
$components = @(
    @('rustc','925e67ef4684861eb301d123a45292a63a0afba49060b60e3264f73c7f9cc1ec'),
    @('cargo','b7806005e41b7733b1f4ec9111b43c2590e1535a7713644660346161cdc71c46'),
    @('rust-std','d82b3240908390cef1ddb88d2d922fa6d5e8abb6e6ef1d47948ec503ba694bf5'),
    @('rust-mingw','3254c05b7a7f8edc915a78b11dafd5395f9bc94b2c6a24016294470190e1f77c')
)
foreach ($component in $components) {
    $name = "$($component[0])-1.90.0-x86_64-pc-windows-gnu"
    $archive = Join-Path $temporary "$name.tar.xz"
    if (-not (Test-Path -LiteralPath $archive)) {
        Write-Output "Download $name"
        Invoke-WebRequest "https://static.rust-lang.org/dist/2025-09-18/$name.tar.xz" -OutFile $archive
    }
    if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLower() -ne $component[1]) { throw "Invalid hash: $archive" }
    & tar.exe -xf $archive -C $temporary
    if ($LASTEXITCODE -ne 0) { throw "Extraction failed: $archive" }
    $payload = Join-Path $temporary $name
    Get-ChildItem -LiteralPath $payload -Directory | ForEach-Object {
        Get-ChildItem -LiteralPath $_.FullName | Where-Object { $_.Name -ne 'manifest.in' } | Copy-Item -Destination $rustRoot -Recurse -Force
    }
}
$kitName = 'w64devkit-x64-2.10.1.7z.exe'
$kitArchive = Join-Path $temporary $kitName
if (-not (Test-Path -LiteralPath $kitArchive)) {
    Invoke-WebRequest "https://github.com/skeeto/w64devkit/releases/download/v2.10.1/$kitName" -OutFile $kitArchive
}
if ((Get-FileHash -LiteralPath $kitArchive -Algorithm SHA256).Hash.ToLower() -ne 'add1edf984ce4d473cfeb7c52e0ab38db73a855bba6c7f564d5e16648eb2896a') { throw 'Invalid w64devkit hash' }
$extract = Start-Process -FilePath $kitArchive -ArgumentList @('-y',"-o$nativeRoot") -WindowStyle Hidden -Wait -PassThru
if ($extract.ExitCode -ne 0) { throw 'w64devkit extraction failed' }
& (Join-Path $rustRoot 'bin\rustc.exe') --version
& (Join-Path $nativeRoot 'w64devkit\bin\gcc.exe') --version
Write-Output 'Local toolchain ready. No registry or permanent PATH changes were made.'
