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
    $PackageId = [guid]::NewGuid().ToString('N')
    $Stage = Join-Path $Root ('dist/package-' + $PackageId)
    $DemoData = Join-Path $Root ('dist/demo-' + $PackageId)
    New-Item -ItemType Directory -Force $Stage | Out-Null
    Copy-Item target/release/copperline-desktop.exe,target/release/copperline-cli.exe,README.md,LICENSE,NOTICE $Stage
    Copy-Item docs -Destination $Stage -Recurse -Force
    Copy-Item legal -Destination $Stage -Recurse -Force
    Copy-Item evidence -Destination $Stage -Recurse -Force
    Copy-Item fixtures -Destination $Stage -Recurse -Force
    New-Item -ItemType Directory (Join-Path $Stage 'scripts') | Out-Null
    Copy-Item scripts/demo_seed.py (Join-Path $Stage 'scripts')
    Copy-Item 'packaging/windows/Start Morning Demo.cmd' $Stage
    python scripts/demo_seed.py --binary target/release/copperline-cli.exe --credits --data $DemoData
    if ($LASTEXITCODE -ne 0) { throw 'Synthetic demo creation failed' }
    $DemoArchive = Join-Path $Stage 'morning-demo.clbak'
    & target/release/copperline-cli.exe --data $DemoData backup $DemoArchive
    if ($LASTEXITCODE -ne 0) { throw 'Synthetic demo backup failed' }
    & target/release/copperline-cli.exe backup-inspect $DemoArchive
    if ($LASTEXITCODE -ne 0) { throw 'Synthetic demo archive validation failed' }
    $RestoreData = Join-Path $Root ('dist/restored-' + $PackageId)
    & target/release/copperline-cli.exe restore $DemoArchive $RestoreData
    if ($LASTEXITCODE -ne 0) { throw 'Synthetic demo restore failed' }
    $OriginalState = & target/release/copperline-cli.exe --data $DemoData state
    if ($LASTEXITCODE -ne 0) { throw 'Synthetic original reopen failed' }
    $RestoredState = & target/release/copperline-cli.exe --data $RestoreData state
    if ($LASTEXITCODE -ne 0) { throw 'Synthetic restored reopen failed' }
    if (($OriginalState -join "`n") -cne ($RestoredState -join "`n")) { throw 'Synthetic restored state differs' }
    if (($RestoredState | ConvertFrom-Json).revision -ne 65) { throw 'Unexpected synthetic demo revision' }
    '@echo off' + "`r`n" + 'cd /d "%~dp0"' + "`r`n" + 'start "" copperline-desktop.exe --demo --data data\demo' | Set-Content -Encoding ascii (Join-Path $Stage 'Start Copperline.cmd')
    $SourceCommit = git rev-parse HEAD
    if ($LASTEXITCODE -ne 0) { throw 'Cannot identify package source' }
    $BuildInfo = @(
        'Copperline unsigned synthetic development package',
        ('Source commit: ' + $SourceCommit),
        ('CI run: ' + $env:GITHUB_SERVER_URL + '/' + $env:GITHUB_REPOSITORY + '/actions/runs/' + $env:GITHUB_RUN_ID),
        'Start Morning Demo.cmd restores the included synthetic revision-65 archive.',
        'No production/full-parity claim; see docs/morning-demo.md and legal/README.md.',
        'Evidence documents can describe earlier milestones; this file identifies these binaries.'
    )
    $BuildInfo | Set-Content -Encoding utf8 (Join-Path $Stage 'BUILD-INFO.txt')
    $Zip = Join-Path $Root 'dist/copperline-0.1.0-windows-x64-portable.zip'
    Compress-Archive -Path "$Stage/*" -DestinationPath $Zip -Force
    Get-FileHash -Algorithm SHA256 $Zip | Format-List
    Write-Host 'Unsigned development build. No installer, registry changes or administrator rights required.'
} finally { Pop-Location }
