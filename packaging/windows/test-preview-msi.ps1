# Builds the preview MSI twice (base + release version) and qualifies silent install with
# options, upgrade over the base version with user data preserved, and a trace-free uninstall.
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$wix = Join-Path $env:WIX 'bin'
$stage = (Resolve-Path dist/msi-stage).Path
$output = (New-Item -ItemType Directory dist/msi-output).FullName
$baseVersion = '0.1.0'
$version = '0.1.1'

function New-Msi($ver, $name) {
    $msi = Join-Path $output $name
    & "$wix/heat.exe" dir $stage -nologo -cg PreviewFiles -dr INSTALLFOLDER -srd -scom -sreg -ag -var var.Stage -out dist/preview-files.wxs
    & "$wix/candle.exe" -nologo -arch x64 "-dStage=$stage" "-dVersion=$ver" -out dist/ packaging/windows/preview.wxs dist/preview-files.wxs
    & "$wix/light.exe" -nologo -out $msi dist/preview.wixobj dist/preview-files.wixobj
    $symbols = [IO.Path]::ChangeExtension($msi, '.wixpdb')
    if (Test-Path $symbols) { Move-Item -Force $symbols dist/ }
    return $msi
}
$baseMsi = New-Msi $baseVersion "RustRemote-Windows-Preview-$baseVersion-x64.msi"
$msi = New-Msi $version "RustRemote-Windows-Preview-$version-x64.msi"

function Invoke-Bounded($path, $arguments, $seconds = 120) {
    $process = Start-Process $path -ArgumentList $arguments -PassThru
    if (-not $process.WaitForExit($seconds * 1000)) {
        Stop-Process -Id $process.Id -Force
        throw "$([IO.Path]::GetFileName($path)) did not finish within $seconds seconds"
    }
    $process.WaitForExit()
    return $process
}
function Invoke-Installer($arguments) {
    $process = Invoke-Bounded "$env:SystemRoot/System32/msiexec.exe" $arguments 300
    if ($process.ExitCode -ne 0) { throw "Windows Installer exited with $($process.ExitCode)" }
}
function Get-Entries {
    Get-ChildItem 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall' |
        ForEach-Object { Get-ItemProperty $_.PSPath } |
        Where-Object { $_.DisplayName -eq 'RustRemote Windows Preview' }
}
function Get-MachinePath { (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment').Path }

$installed = Join-Path $env:ProgramFiles 'RustRemote Windows Preview'
$menu = Join-Path $env:ProgramData 'Microsoft\Windows\Start Menu\Programs\RustRemote Windows Preview'
$desktop = Join-Path $env:Public 'Desktop\RustRemote.lnk'
$userData = Join-Path $env:LOCALAPPDATA 'RustRemote'
if (Test-Path $installed) { throw 'Preview installation path already exists' }
$env:Path = "$env:SystemRoot\System32;$env:SystemRoot"
$pathBefore = Get-MachinePath
try {
    # 1. Silent install of the base version with every option.
    Invoke-Installer "/i `"$baseMsi`" /qn /norestart DESKTOPSHORTCUT=1 ADDTOPATH=1 /l*v `"$output/install-base.log`""
    $exe = Join-Path $installed 'rustremote-windows-probe.exe'
    if (-not (Test-Path (Join-Path $menu 'RustRemote.lnk'))) { throw 'Start menu shortcut was not installed' }
    if (-not (Test-Path $desktop)) { throw 'DESKTOPSHORTCUT=1 did not create the desktop shortcut' }
    if (-not ((Get-MachinePath).Split(';') -contains $installed)) { throw 'ADDTOPATH=1 did not extend the system PATH' }
    $rdp = Join-Path $installed 'wfreerdp.exe'
    $rdpProcess = Invoke-Bounded $rdp '/version'
    if ($rdpProcess.ExitCode -ne 0) { throw "Installed FreeRDP exited with $($rdpProcess.ExitCode)" }
    $process = Invoke-Bounded $exe '--smoke-ui'
    if ($process.ExitCode -ne 0) { throw "Installed application exited with $($process.ExitCode)" }

    # 2. User data created by the user must survive an upgrade.
    New-Item -ItemType Directory -Force $userData | Out-Null
    Set-Content -Path (Join-Path $userData 'upgrade-marker.txt') -Value 'keep'

    # 3. Silent in-place upgrade to the release version (no options: they must not be required).
    Invoke-Installer "/i `"$msi`" /qn /norestart /l*v `"$output/install.log`""
    $entries = @(Get-Entries)
    if ($entries.Count -ne 1 -or $entries[0].DisplayVersion -ne $version) { throw "Upgrade left $($entries.Count) entries, version $($entries[0].DisplayVersion)" }
    if (-not (Test-Path (Join-Path $userData 'upgrade-marker.txt'))) { throw 'Upgrade removed user data' }
    if (-not (Test-Path $exe)) { throw 'Upgrade removed the application' }
    $process = Invoke-Bounded $exe '--smoke-ui'
    if ($process.ExitCode -ne 0) { throw "Upgraded application exited with $($process.ExitCode)" }
} finally {
    foreach ($entry in @(Get-Entries)) {
        Invoke-Installer "/x $($entry.PSChildName) /qn /norestart REMOVEUSERDATA=1 /l*v `"$output/uninstall.log`""
    }
}
# 4. No trace after uninstall.
foreach ($left in @($installed, $menu, $desktop, $userData)) {
    if (Test-Path $left) { throw "Uninstall left $left" }
}
if (@(Get-Entries).Count -ne 0) { throw 'Uninstall left an Add/Remove Programs entry' }
if (Test-Path 'HKCU:\Software\RustRemote') { throw 'Uninstall left the per-user registry key' }
if ((Get-MachinePath) -ne $pathBefore) { throw 'Uninstall left a modified system PATH' }
Remove-Item (Join-Path $output 'RustRemote-Windows-Preview-0.1.0-x64.msi') -Force
$hash = (Get-FileHash -Algorithm SHA256 $msi).Hash.ToLowerInvariant()
"$hash  $([IO.Path]::GetFileName($msi))" | Set-Content -Encoding ascii "$msi.sha256"
