param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path "$PSScriptRoot/../..").Path
Push-Location $Root
try {
    if (-not $SkipBuild) {
        cargo xtask ci
        if ($LASTEXITCODE -ne 0) { throw 'Quality gates failed' }
        cargo build --locked --release -p copperline-cli -p copperline-desktop
        if ($LASTEXITCODE -ne 0) { throw 'Build failed' }
    }
    $Stage = Join-Path $Root 'dist/copperline-windows-x64'
    New-Item -ItemType Directory -Force $Stage | Out-Null
    Copy-Item target/release/copperline-desktop.exe,target/release/copperline-cli.exe,README.md,LICENSE,NOTICE $Stage
    Copy-Item docs -Destination $Stage -Recurse -Force
    Copy-Item legal -Destination $Stage -Recurse -Force
    '@echo off' + "`r`n" + 'cd /d "%~dp0"' + "`r`n" + 'start "" copperline-desktop.exe --demo --data data\demo' | Set-Content -Encoding ascii (Join-Path $Stage 'Start Copperline.cmd')
    $Zip = Join-Path $Root 'dist/copperline-0.1.0-windows-x64-portable.zip'
    Compress-Archive -Path "$Stage/*" -DestinationPath $Zip -Force
    Get-FileHash -Algorithm SHA256 $Zip | Format-List
    Write-Host 'Unsigned development build. No installer, registry changes or administrator rights required.'
} finally { Pop-Location }
