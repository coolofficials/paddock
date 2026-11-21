param([Parameter(ValueFromRemainingArguments=$true)][string[]]$CargoArguments)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$variables = @('PATH','CARGO_HOME','RUSTUP_HOME','CARGO_TARGET_DIR','TMP','TEMP','RUSTC','RUSTDOC','RUSTFLAGS','CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER')
$previous = @{}
foreach ($variable in $variables) { $previous[$variable] = [Environment]::GetEnvironmentVariable($variable,'Process') }
Push-Location $projectRoot
try {
    if (-not (Test-Path -LiteralPath (Join-Path $projectRoot ".dev\rust\bin\cargo.exe"))) {
        & cargo @CargoArguments
        if ($LASTEXITCODE -ne 0) { throw "Cargo failed ($LASTEXITCODE)" }
        return
    }
    $env:CARGO_HOME = Join-Path $projectRoot '.dev\cargo'
    $env:RUSTUP_HOME = Join-Path $projectRoot '.dev\rustup'
    $env:CARGO_TARGET_DIR = Join-Path $projectRoot 'target'
    $env:TEMP = Join-Path $projectRoot '.dev\temp'
    $env:TMP = $env:TEMP
    $env:RUSTC = Join-Path $projectRoot '.dev\rust\bin\rustc.exe'
    $env:RUSTDOC = Join-Path $projectRoot '.dev\rust\bin\rustdoc.exe'
    $env:RUSTFLAGS = '-C link-self-contained=yes'
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = Join-Path $projectRoot '.dev\native\w64devkit\bin\gcc.exe'
    $env:PATH = "$(Join-Path $projectRoot '.dev\rust\bin');$(Join-Path $projectRoot '.dev\native\w64devkit\bin');$env:PATH"
    & (Join-Path $projectRoot '.dev\rust\bin\cargo.exe') @CargoArguments
    if ($LASTEXITCODE -ne 0) { throw "Cargo failed ($LASTEXITCODE)" }
} finally {
    Pop-Location
    foreach ($variable in $variables) { [Environment]::SetEnvironmentVariable($variable,$previous[$variable],'Process') }
}
