param([switch]$UseSystemCargo, [string]$OutputDirectory = 'dist\D101-Label-Studio')
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
$priorFlags = $env:CARGO_ENCODED_RUSTFLAGS
try {
    $flags = @()
    if ($priorFlags) { $flags += $priorFlags.Split([char]31) }
    if (-not $UseSystemCargo) { $flags += @('-C','linker-flavor=ld.lld','-C','link-self-contained=yes') }
    foreach ($entry in @(@($env:USERPROFILE,'/user'), @($root,'/studio'))) {
        $path = $entry[0]
        if ($path) {
            $flags += "--remap-path-prefix=$path=$($entry[1])"
            $flags += "--remap-path-prefix=\\?\$path=$($entry[1])"
        }
    }
    $env:CARGO_ENCODED_RUSTFLAGS = $flags -join [char]31
    if ($UseSystemCargo) { & cargo build --release --locked } else { & powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/cargo.ps1 build --release --locked -j 4 }
    if ($LASTEXITCODE -ne 0) { throw 'Build failed' }
    $output = [IO.Path]::GetFullPath((Join-Path $root $OutputDirectory))
    New-Item -ItemType Directory -Force (Join-Path $root 'dist') | Out-Null
    New-Item -ItemType Directory -Force $output,(Join-Path $output 'scripts'),(Join-Path $output 'examples') | Out-Null
    Copy-Item -LiteralPath (Join-Path $root 'target\release\niimbot-d101.exe') -Destination $output
    foreach ($name in @('README.md','THIRD_PARTY_NOTICES.md','THIRD_PARTY_LICENSES.txt','LICENSE')) { Copy-Item -LiteralPath (Join-Path $root $name) -Destination $output }
    New-Item -ItemType Directory -Force (Join-Path $output 'docs') | Out-Null
    foreach ($name in @('D101-MEDIA.md','guide-ru.md','guide-en.md')) {
        Copy-Item -LiteralPath (Join-Path $root "docs\$name") -Destination (Join-Path $output 'docs')
    }
    foreach ($name in @('Install-Printer.ps1','Uninstall-Printer.ps1')) { Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination (Join-Path $output 'scripts') }
    Copy-Item -LiteralPath (Join-Path $root 'examples\labels.csv') -Destination (Join-Path $output 'examples')
    Copy-Item -LiteralPath (Join-Path $root 'examples\generator.json') -Destination (Join-Path $output 'examples')
    # Include only public release files, even if someone kept a project next to the executable.
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = Join-Path $root 'dist\D101-Label-Studio-win11-x64.zip'
    $tempZip = "$archive.$([Guid]::NewGuid()).tmp"
    $zip = [IO.Compression.ZipFile]::Open($tempZip, 'Create')
    try {
        foreach ($file in @('niimbot-d101.exe','README.md','THIRD_PARTY_NOTICES.md','THIRD_PARTY_LICENSES.txt','LICENSE',
            'docs/D101-MEDIA.md','docs/guide-ru.md','docs/guide-en.md','scripts/Install-Printer.ps1','scripts/Uninstall-Printer.ps1',
            'examples/labels.csv','examples/generator.json')) {
            [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip,(Join-Path $output $file),$file) | Out-Null
        }
    } finally { $zip.Dispose() }
    if ([IO.File]::Exists($archive)) {
        [IO.File]::Replace($tempZip,$archive,[System.Management.Automation.Language.NullString]::Value)
    }
    else { [IO.File]::Move($tempZip,$archive) }
    Get-FileHash -LiteralPath (Join-Path $output 'niimbot-d101.exe') -Algorithm SHA256
} finally {
    $env:CARGO_ENCODED_RUSTFLAGS = $priorFlags
    Pop-Location
}
