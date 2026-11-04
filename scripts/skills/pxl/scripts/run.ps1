# Launcher for the bundled pipeline binaries (Windows PowerShell 5.1+ / pwsh).
#
#   run.ps1 <step-100|step-200|step-300|step-400|step-500> [args...]
#   run.ps1 --platform     print the detected platform and exit
#   run.ps1 --verify       check bin\manifest.json hashes for this platform
$ErrorActionPreference = 'Stop'

function Fail([string]$msg) { [Console]::Error.WriteLine("run.ps1: $msg"); exit 127 }

$skill = Split-Path -Parent $PSScriptRoot
$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -eq 'AMD64') { $platform = 'windows-x64' } else { Fail "unsupported architecture '$arch' (only windows-x64 is bundled)" }
$binDir = Join-Path (Join-Path $skill 'bin') $platform

if ($args.Count -eq 0) { Fail 'usage: run.ps1 <step-100|step-200|step-300|step-400|step-500> [args...]' }
$first = $args[0]

if ($first -eq 'serve') {
    if ($args.Count -lt 2) { Fail 'usage: run.ps1 serve <dir> [--port N]' }
    $dir = $args[1]
    $port = 8000
    if ($args.Count -ge 4 -and $args[2] -eq '--port') { $port = $args[3] }
    if (-not (Test-Path $dir -PathType Container)) { Fail "serve directory '$dir' does not exist" }
    & python3 -m http.server $port --directory $dir
    exit $LASTEXITCODE
}

if ($first -eq '--platform') { Write-Output $platform; exit 0 }

if ($first -eq '--verify') {
    $manifestPath = Join-Path (Join-Path $skill 'bin') 'manifest.json'
    if (-not (Test-Path $manifestPath)) { Fail 'bin\manifest.json is missing' }
    $manifest = Get-Content $manifestPath -Raw | ConvertFrom-Json
    $entry = $manifest.platforms.$platform
    if (-not $entry) { Fail "manifest has no entry for $platform" }
    $bad = 0
    foreach ($p in $entry.PSObject.Properties) {
        $path = Join-Path $binDir $p.Name
        if (-not (Test-Path $path)) { Write-Output "MISSING $($p.Name)"; $bad++; continue }
        $h = 'sha256:' + (Get-FileHash $path -Algorithm SHA256).Hash.ToLower()
        if ($h -ne $p.Value) { Write-Output "MISMATCH $($p.Name)"; $bad++ } else { Write-Output "ok $($p.Name)" }
    }
    exit $(if ($bad) { 1 } else { 0 })
}

$step = $first
$rest = @()
if ($args.Count -gt 1) { $rest = $args[1..($args.Count - 1)] }
if ($step -notin @('step-100', 'step-200', 'step-300', 'step-400', 'step-500')) {
    Fail "unknown step '$step' (expected step-100 .. step-500)"
}

$exe = Join-Path $binDir "$step.exe"
if (-not (Test-Path $exe)) {
    $have = ''
    $binRoot = Join-Path $skill 'bin'
    if (Test-Path $binRoot) { $have = (Get-ChildItem $binRoot -Directory | ForEach-Object Name) -join ' ' }
    Fail "no $step binary for platform '$platform' (bundled platforms: $have). Build one with: scripts/build-skill-bins.sh --target $platform"
}

$env:PATH = "$binDir;$env:PATH"
if ($step -eq 'step-300' -and $rest.Count -gt 0 -and $rest[0] -in @('audio', 'plan') -and -not $env:PXL_AUDIO_ADAPTER) {
    $hasAdapter = $false
    foreach ($a in $rest) { if ($a -eq '--adapter' -or $a -like '--adapter=*') { $hasAdapter = $true } }
    if (-not $hasAdapter) { $env:PXL_AUDIO_ADAPTER = Join-Path $skill 'assets/audio-toolchain' }
}
if ($step -eq 'step-200' -and $rest.Count -gt 0 -and $rest[0] -in @('run', 'lint', 'capabilities')) {
    $hasBin = $false
    foreach ($a in $rest) { if ($a -eq '--pxl-bin' -or $a -like '--pxl-bin=*') { $hasBin = $true } }
    if (-not $hasBin) { $rest += @('--pxl-bin', (Join-Path $binDir 'step-300.exe')) }
}
if ($step -eq 'step-500' -and $rest.Count -gt 0 -and $rest[0] -eq 'build') {
    $hasRuntime = $false
    foreach ($a in $rest) { if ($a -eq '--runtime-dir' -or $a -like '--runtime-dir=*') { $hasRuntime = $true } }
    if (-not $hasRuntime) { $rest += @('--runtime-dir', (Join-Path $skill 'assets/web-runtime')) }
}
& $exe @rest
exit $LASTEXITCODE
