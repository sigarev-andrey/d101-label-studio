$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$toolsDir = Join-Path $root '.tools'
New-Item -ItemType Directory -Force $toolsDir | Out-Null
$installer = Join-Path $toolsDir 'rustup-init.exe'
Invoke-WebRequest -Uri 'https://win.rustup.rs/x86_64' -OutFile $installer
$env:RUSTUP_HOME = Join-Path $toolsDir 'rustup'
$env:CARGO_HOME = Join-Path $toolsDir 'cargo'
& $installer -y --no-modify-path --default-host x86_64-pc-windows-gnu --profile minimal --component rustfmt,clippy
if ($LASTEXITCODE -ne 0) { throw 'Rust installation failed' }
$release = Invoke-RestMethod -Uri 'https://api.github.com/repos/mstorsjo/llvm-mingw/releases/tags/20261104'
$asset = $release.assets | Where-Object name -EQ 'llvm-mingw-20261104-ucrt-x86_64.zip' | Select-Object -First 1
if (-not $asset -or -not $asset.digest -or -not $asset.digest.StartsWith('sha256:')) { throw 'No verified LLVM-MinGW release found' }
$archive = Join-Path $toolsDir 'llvm-mingw.zip'
Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $archive
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLower() -ne $asset.digest.Substring(7)) { throw 'LLVM-MinGW checksum mismatch' }
Expand-Archive -LiteralPath $archive -DestinationPath $toolsDir -Force
Write-Host 'Portable tools are ready. System PATH was not modified.'
