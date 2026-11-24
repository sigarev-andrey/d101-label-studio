#Requires -RunAsAdministrator
param(
    [string]$Name = 'Niimbot D101',
    [ValidateRange(1,65535)][int]$Port = 8631
)
$ErrorActionPreference = 'Stop'
Import-Module PrintManagement
$url = "http://127.0.0.1:$Port/ipp/print"
try {
    $health = Invoke-WebRequest -Uri "http://127.0.0.1:$Port/health" -UseBasicParsing -TimeoutSec 5
    if ($health.Content -notmatch 'Niimbot D101 IPP bridge') { throw 'Unexpected service on this port' }
} catch { throw "Start D101 Label Studio before installing the printer. $($_.Exception.Message)" }
$existing = Get-Printer -Name $Name -ErrorAction SilentlyContinue
if ($existing) {
    throw "Printer '$Name' already exists. No changes made. Use another -Name or remove that queue explicitly."
}
Add-Printer -Name $Name -IppURL $url
$printer = Get-Printer -Name $Name -ErrorAction Stop
if ($printer.DriverName -ne 'Microsoft IPP Class Driver') {
    Write-Warning "Windows selected '$($printer.DriverName)'. Verify PWG Raster support before printing."
}
$printer | Select-Object Name,DriverName,PortName
Write-Host 'Printer installed. Keep D101 Label Studio running. Select the label media size, then test ONE label.'
