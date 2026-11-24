#Requires -RunAsAdministrator
param([string]$Name = 'Niimbot D101')
$ErrorActionPreference = 'Stop'
Import-Module PrintManagement
$printer = Get-Printer -Name $Name -ErrorAction Stop
if ($printer.DriverName -ne 'Microsoft IPP Class Driver') {
    throw "Refusing to remove '$Name': this is not a Microsoft IPP Class Driver queue."
}
Remove-Printer -Name $Name -Confirm
