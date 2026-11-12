# Removes the Halo Island test copy and stops trusting its certificate,
# then starts the usual Halo Island again if it's installed. README.txt has
# the whole story. Windows PowerShell 5.1.
param([switch]$UntrustOnly)

$ErrorActionPreference = "Stop"
$cer = Join-Path $PSScriptRoot "halo-island-test.cer"
$thumb = if (Test-Path $cer) { (New-Object System.Security.Cryptography.X509Certificates.X509Certificate2 $cer).Thumbprint } else { "" }

if ($UntrustOnly) {
    # The elevated half: forget the certificate, nothing else.
    if ($thumb) { Remove-Item "Cert:\LocalMachine\TrustedPeople\$thumb" -ErrorAction SilentlyContinue }
    exit 0
}

$pkg = Get-AppxPackage -Name "Kolijijani.HaloIsland"
if ($pkg) {
    "Removing the test copy ($($pkg.Version)) and its settings."
    Remove-AppxPackage -Package $pkg.PackageFullName
} else {
    "The test copy isn't installed."
}

if ($thumb -and (Test-Path "Cert:\LocalMachine\TrustedPeople\$thumb")) {
    "Windows will ask for administrator rights, to stop trusting the test copy's certificate."
    Start-Process powershell -Verb RunAs -Wait -ArgumentList "-NoProfile -ExecutionPolicy Bypass -File `"$PSCommandPath`" -UntrustOnly" | Out-Null
}

$usual = Join-Path $env:LOCALAPPDATA "Programs\Halo\halo.exe"
if (Test-Path $usual) {
    "Starting your usual Halo Island."
    Start-Process $usual
}
