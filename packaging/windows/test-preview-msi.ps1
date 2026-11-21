$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$wix = Join-Path $env:WIX 'bin'
$stage = (Resolve-Path dist/msi-stage).Path
$output = (New-Item -ItemType Directory dist/msi-output).FullName
$msi = Join-Path $output 'RustRemote-Windows-Preview-0.1.0-x64.msi'

& "$wix/heat.exe" dir $stage -nologo -cg PreviewFiles -dr INSTALLFOLDER -srd -scom -sreg -ag -var var.Stage -out dist/preview-files.wxs
& "$wix/candle.exe" -nologo -arch x64 "-dStage=$stage" -out dist/ packaging/windows/preview.wxs dist/preview-files.wxs
& "$wix/light.exe" -nologo -out $msi dist/preview.wixobj dist/preview-files.wixobj
$symbols = [IO.Path]::ChangeExtension($msi, '.wixpdb')
if (Test-Path $symbols) { Move-Item $symbols dist/ }

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

$installed = Join-Path $env:ProgramFiles 'RustRemote Windows Preview'
if (Test-Path $installed) { throw 'Preview installation path already exists' }
$env:Path = "$env:SystemRoot\System32;$env:SystemRoot"
try {
    Invoke-Installer "/i `"$msi`" /qn /norestart /l*v `"$output/install.log`""
    $exe = Join-Path $installed 'rustremote-windows-probe.exe'
    $rdp = Join-Path $installed 'wfreerdp.exe'
    $rdpProcess = Invoke-Bounded $rdp '/version'
    if ($rdpProcess.ExitCode -ne 0) { throw "Installed FreeRDP exited with $($rdpProcess.ExitCode)" }
    $process = Invoke-Bounded $exe '--smoke-ui'
    if ($process.ExitCode -ne 0) { throw "Installed application exited with $($process.ExitCode)" }
} finally {
    if (Test-Path $installed) {
        Invoke-Installer "/x `"$msi`" /qn /norestart /l*v `"$output/uninstall.log`""
    }
}
if (Test-Path $installed) { throw 'Uninstall left the application directory' }
$hash = (Get-FileHash -Algorithm SHA256 $msi).Hash.ToLowerInvariant()
"$hash  $([IO.Path]::GetFileName($msi))" | Set-Content -Encoding ascii "$msi.sha256"
