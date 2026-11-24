$CargoArgs = @($args)
# PowerShell consumes the standalone -- token when invoking a script.
# Restore Cargo's separator for compiler/linter options.
if ($CargoArgs.Count -gt 0 -and $CargoArgs[0] -eq 'clippy' -and -not ($CargoArgs -contains '--')) {
    for ($i = 1; $i -lt $CargoArgs.Count; $i++) {
        if ($CargoArgs[$i] -match '^-[DAWF]') {
            $CargoArgs = @($CargoArgs[0..($i-1)]) + @('--') + @($CargoArgs[$i..($CargoArgs.Count-1)])
            break
        }
    }
}
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$env:RUSTUP_HOME = Join-Path $root '.tools\rustup'
$env:CARGO_HOME = Join-Path $root '.tools\cargo'
$env:PATH = (Join-Path $env:CARGO_HOME 'bin') + ';' + $env:PATH
$toolchain = Join-Path $env:RUSTUP_HOME 'toolchains\stable-x86_64-pc-windows-gnu'
$env:PATH = (Join-Path $toolchain 'lib\rustlib\x86_64-pc-windows-gnu\bin') + ';' + $env:PATH
$env:PATH = (Join-Path $toolchain 'lib\rustlib\x86_64-pc-windows-gnu\bin\self-contained') + ';' + $env:PATH
$llvm = Get-ChildItem -LiteralPath (Join-Path $root '.tools') -Directory -Filter 'llvm-mingw-*-ucrt-x86_64' | Sort-Object Name -Descending | Select-Object -First 1
if ($llvm) {
    $llvmBin = Join-Path $llvm.FullName 'bin'
    if (-not (Test-Path -LiteralPath (Join-Path $llvmBin 'dlltool.exe'))) { Copy-Item -LiteralPath (Join-Path $llvmBin 'llvm-dlltool.exe') -Destination (Join-Path $llvmBin 'dlltool.exe') }
    $env:PATH = $llvmBin + ';' + $env:PATH
}
$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = 'rust-lld.exe'
$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS = '-C linker-flavor=ld.lld -C link-self-contained=yes'
Push-Location $root
try { & (Join-Path $env:CARGO_HOME 'bin\cargo.exe') @CargoArgs; exit $LASTEXITCODE } finally { Pop-Location }
