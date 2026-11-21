param([Parameter(ValueFromRemainingArguments=$true)][string[]]$CargoArguments)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$variables = @('PATH','CARGO_HOME','RUSTUP_HOME','CARGO_TARGET_DIR','TMP','TEMP','RUSTC','RUSTDOC','RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER')
$previous = @{}
foreach ($variable in $variables) { $previous[$variable] = [Environment]::GetEnvironmentVariable($variable,'Process') }
Push-Location $projectRoot
try {
    $portable = Test-Path -LiteralPath (Join-Path $projectRoot '.dev\rust\bin\cargo.exe')
    $cargoRoot = if ($portable) { Join-Path $projectRoot '.dev\cargo' } elseif ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
    $rustupRoot = if ($portable) { Join-Path $projectRoot '.dev\rustup' } elseif ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { Join-Path $env:USERPROFILE '.rustup' }
    # Encoded arguments preserve spaces in Windows paths. Apply the most specific
    # mappings last, so file!() and panic locations contain portable virtual paths.
    $compilerFlags = @()
    if ($env:CARGO_ENCODED_RUSTFLAGS) { $compilerFlags += $env:CARGO_ENCODED_RUSTFLAGS.Split([char]31) }
    elseif ($env:RUSTFLAGS) { throw 'Use CARGO_ENCODED_RUSTFLAGS instead of RUSTFLAGS when using this build wrapper.' }
    if ($portable) { $compilerFlags += @('-C', 'link-self-contained=yes') }
    $mappings = @(@($env:USERPROFILE, '/build-home'), @($projectRoot, '/paddock'), @($cargoRoot, '/rust-deps'), @($rustupRoot, '/rust-toolchain'))
    foreach ($mapping in $mappings) {
        if ($mapping[0]) {
            $compilerFlags += '--remap-path-prefix=' + $mapping[0] + '=' + $mapping[1]
            $compilerFlags += '--remap-path-prefix=' + $mapping[0].Replace('\','/') + '=' + $mapping[1]
        }
    }
    $env:RUSTFLAGS = $null
    $env:CARGO_ENCODED_RUSTFLAGS = $compilerFlags -join [char]31
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
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = Join-Path $projectRoot '.dev\native\w64devkit\bin\gcc.exe'
    $env:PATH = "$(Join-Path $projectRoot '.dev\rust\bin');$(Join-Path $projectRoot '.dev\native\w64devkit\bin');$env:PATH"
    & (Join-Path $projectRoot '.dev\rust\bin\cargo.exe') @CargoArguments
    if ($LASTEXITCODE -ne 0) { throw "Cargo failed ($LASTEXITCODE)" }
} finally {
    Pop-Location
    foreach ($variable in $variables) { [Environment]::SetEnvironmentVariable($variable,$previous[$variable],'Process') }
}
