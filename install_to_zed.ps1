[CmdletBinding()]
param(
    [string]$ZedExtensionsDir,
    [string]$RunnerBinDir,
    [switch]$SkipSettings,
    [switch]$AllowMissingHttpyac
)

$ErrorActionPreference = 'Stop'
$ProjectDir = Split-Path -Parent $MyInvocation.MyCommand.Path

function Get-RequiredCommand {
    param([string]$Name)

    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if ($null -eq $command) {
        throw "Missing required command: $Name"
    }
    return $command.Source
}

function Get-OptionalCommand {
    param([string]$Name)

    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if ($null -eq $command) {
        return $null
    }
    return $command.Source
}

function Invoke-Checked {
    param(
        [string]$File,
        [string[]]$Arguments,
        [string]$Description
    )

    & $File @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Description failed (exit code $LASTEXITCODE)"
    }
}

function Copy-ExtensionPayload {
    param(
        [string]$Destination,
        [string]$WasmSource,
        [string]$LspSource,
        [string]$RunnerSource
    )

    New-Item -ItemType Directory -Force -Path (Join-Path $Destination 'grammars') | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $Destination 'lsp\httpyac-models') | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $Destination 'languages') | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $Destination 'lsp\httpyac-models\src') | Out-Null
    Copy-Item -Force (Join-Path $ProjectDir 'extension.toml') $Destination
    Copy-Item -Force $WasmSource (Join-Path $Destination 'extension.wasm')
    Copy-Item -Recurse -Force (Join-Path (Join-Path $ProjectDir 'languages') '*') (Join-Path $Destination 'languages')
    Copy-Item -Force (Join-Path $ProjectDir 'grammars\http.wasm') (Join-Path $Destination 'grammars\http.wasm')
    if (Test-Path (Join-Path $ProjectDir 'snippets')) {
        Copy-Item -Recurse -Force (Join-Path $ProjectDir 'snippets') (Join-Path $Destination 'snippets')
    }
    Copy-Item -Force $LspSource (Join-Path $Destination 'lsp\httpyac-lsp.exe')
    Copy-Item -Force $RunnerSource (Join-Path $Destination 'lsp\httpyac-run.exe')
    Copy-Item -Force (Join-Path $ProjectDir 'lsp\httpyac-models\httpyac-globals.d.ts') (Join-Path $Destination 'lsp\httpyac-models\httpyac-globals.d.ts')
    Copy-Item -Recurse -Force (Join-Path (Join-Path $ProjectDir 'lsp\httpyac-models\src') '*') (Join-Path $Destination 'lsp\httpyac-models\src')
}

function Add-UserPathEntry {
    param([string]$PathEntry)

    $current = [Environment]::GetEnvironmentVariable('Path', 'User')
    $entries = @($current -split ';' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    if ($entries -contains $PathEntry) {
        return $false
    }
    $newPath = (@($entries) + $PathEntry) -join ';'
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    return $true
}

function New-SettingsObject {
    param(
        [string]$HttpyacPath,
        [string]$LspPath,
        [string]$VtslsPath
    )

    return [ordered]@{
        httpyac = [ordered]@{
            command = $HttpyacPath
            lsp_command = $LspPath
            vtsls_command = $VtslsPath
            default_env = 'dev'
        }
        lsp = [ordered]@{
            'httpyac-lsp' = [ordered]@{
                binary = [ordered]@{
                    path = $LspPath
                    arguments = @()
                }
                settings = [ordered]@{
                    vtslsCommand = $VtslsPath
                }
            }
        }
        languages = [ordered]@{
            HTTP = [ordered]@{
                enable_language_server = $true
                language_servers = @('httpyac-lsp')
                completions = [ordered]@{
                    lsp = $true
                    words = 'fallback'
                    words_min_length = 3
                }
            }
        }
    }
}

if ([string]::IsNullOrWhiteSpace($env:LOCALAPPDATA) -or [string]::IsNullOrWhiteSpace($env:APPDATA)) {
    throw 'LOCALAPPDATA or APPDATA is not set; unable to locate the Windows Zed installation directory.'
}

$cargo = Get-RequiredCommand 'cargo'
$rustup = Get-RequiredCommand 'rustup'
$httpyacPath = Get-OptionalCommand 'httpyac'
$vtslsPath = Get-OptionalCommand 'vtsls'

Write-Host '=========================================='
Write-Host ' HTTPyac Client - Build and Install for Zed (Windows)'
Write-Host '=========================================='

$installedTargets = & $rustup target list --installed
if ($LASTEXITCODE -ne 0) {
    throw 'Unable to list installed Rust targets.'
}
if ($installedTargets -notcontains 'wasm32-wasip2') {
    Invoke-Checked $rustup @('target', 'add', 'wasm32-wasip2') 'Install wasm32-wasip2'
}

Invoke-Checked $cargo @('build', '--release', '--manifest-path', (Join-Path $ProjectDir 'lsp\Cargo.toml'), '--bins') 'Build httpyac-lsp/httpyac-run'
Invoke-Checked $cargo @('build', '--release', '--manifest-path', (Join-Path $ProjectDir 'Cargo.toml'), '--target', 'wasm32-wasip2') 'Build WASM extension'

$wasmCandidates = @(
    (Join-Path $ProjectDir 'target\wasm32-wasip2\release\zed_httpyacclient.wasm'),
    (Join-Path $ProjectDir 'target\wasm32-wasip2\release\httpyacclient.wasm')
)
$wasmSource = $wasmCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if ($null -eq $wasmSource) {
    throw 'WASM extension artifact not found.'
}
Copy-Item -Force $wasmSource (Join-Path $ProjectDir 'extension.wasm')

$grammarPath = Join-Path $ProjectDir 'grammars\http.wasm'
if (-not (Test-Path $grammarPath)) {
    throw "Grammar file is missing: $grammarPath. Generate and commit it on macOS/Linux before running the Windows installer."
}

$lspSource = Join-Path $ProjectDir 'lsp\target\release\httpyac-lsp.exe'
$runnerSource = Join-Path $ProjectDir 'lsp\target\release\httpyac-run.exe'
if (-not (Test-Path $lspSource) -or -not (Test-Path $runnerSource)) {
    throw 'Windows LSP or task runner artifact not found.'
}

if ([string]::IsNullOrWhiteSpace($ZedExtensionsDir)) {
    $ZedExtensionsDir = Join-Path $env:LOCALAPPDATA 'Zed\extensions\installed'
}
if ([string]::IsNullOrWhiteSpace($RunnerBinDir)) {
    $RunnerBinDir = Join-Path $env:LOCALAPPDATA 'httpyacclient\bin'
}

$installDir = Join-Path $ZedExtensionsDir 'httpyacclient'
$stageDir = Join-Path $ZedExtensionsDir ".httpyacclient.stage.$PID"
$backupDir = $null

if (Test-Path $stageDir) {
    throw "Staging directory already exists: $stageDir"
}

try {
    New-Item -ItemType Directory -Force -Path $ZedExtensionsDir | Out-Null
    Copy-ExtensionPayload $stageDir $wasmSource $lspSource $runnerSource

    $requiredFiles = @(
        (Join-Path $stageDir 'extension.toml'),
        (Join-Path $stageDir 'extension.wasm'),
        (Join-Path $stageDir 'grammars\http.wasm'),
        (Join-Path $stageDir 'lsp\httpyac-lsp.exe'),
        (Join-Path $stageDir 'lsp\httpyac-run.exe')
    )
    if ($requiredFiles | Where-Object { -not (Test-Path $_) }) {
        throw 'Staged extension is incomplete; existing installation was not replaced.'
    }

    if (Test-Path $installDir) {
        $backupDir = "$installDir.backup.$PID"
        Move-Item -Force $installDir $backupDir
    }
    Move-Item -Force $stageDir $installDir

    if ($null -ne $backupDir -and (Test-Path $backupDir)) {
        Remove-Item -Recurse -Force $backupDir
        $backupDir = $null
    }
}
catch {
    if ($null -ne $backupDir -and (Test-Path $backupDir) -and -not (Test-Path $installDir)) {
        Move-Item -Force $backupDir $installDir
        $backupDir = $null
    }
    throw
}
finally {
    if (Test-Path $stageDir) {
        Remove-Item -Recurse -Force $stageDir
    }
}

New-Item -ItemType Directory -Force -Path $RunnerBinDir | Out-Null
Copy-Item -Force $lspSource (Join-Path $RunnerBinDir 'httpyac-lsp.exe')
Copy-Item -Force $runnerSource (Join-Path $RunnerBinDir 'httpyac-run.exe')
$runnerModelsDir = Join-Path $RunnerBinDir 'httpyac-models'
New-Item -ItemType Directory -Force -Path (Join-Path $runnerModelsDir 'src') | Out-Null
Copy-Item -Force (Join-Path $ProjectDir 'lsp\httpyac-models\httpyac-globals.d.ts') (Join-Path $runnerModelsDir 'httpyac-globals.d.ts')
Copy-Item -Recurse -Force (Join-Path (Join-Path $ProjectDir 'lsp\httpyac-models\src') '*') (Join-Path $runnerModelsDir 'src')

$pathUpdated = Add-UserPathEntry $RunnerBinDir
$lspPath = Join-Path $installDir 'lsp\httpyac-lsp.exe'
$effectiveHttpyacPath = if ($null -eq $httpyacPath) { 'httpyac' } else { $httpyacPath }
$effectiveVtslsPath = if ($null -eq $vtslsPath) { 'vtsls' } else { $vtslsPath }
$settingsObject = New-SettingsObject $effectiveHttpyacPath $lspPath $effectiveVtslsPath
$settingsPath = Join-Path $env:APPDATA 'Zed\settings.json'

if ($SkipSettings) {
    Write-Host "Skipped settings.json: $settingsPath"
}
elseif (-not (Test-Path $settingsPath)) {
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $settingsPath) | Out-Null
    $settingsObject | ConvertTo-Json -Depth 10 | Set-Content -Encoding UTF8 $settingsPath
    Write-Host "Created settings.json: $settingsPath"
}
else {
    Write-Warning "Existing settings.json contains JSONC comments; it was not modified to preserve them: $settingsPath"
    Write-Host 'Merge the following object into the top level of settings.json:'
    $settingsObject | ConvertTo-Json -Depth 10
}

Write-Host ''
Write-Host "Extension directory: $installDir"
Write-Host "Runner directory: $RunnerBinDir"
Write-Host "LSP path: $lspPath"
if ($pathUpdated) {
    Write-Host 'The runner directory was added to the user PATH. Fully quit and reopen Zed for it to take effect.'
}
else {
    Write-Host 'The runner directory is already in the user PATH. Fully restart Zed to load the new extension.'
}

if ($null -eq $httpyacPath) {
    Write-Warning 'httpyac was not found. The extension is installed, but Send cannot run. Run npm install -g httpyac and restart Zed.'
    if (-not $AllowMissingHttpyac) {
        exit 2
    }
}
if ($null -eq $vtslsPath) {
    Write-Warning 'vtsls was not found. Script blocks will not have full Node/JS or official httpyac model completions. Run npm install -g @vtsls/language-server.'
}

Write-Host 'Installation complete. Fully quit and reopen Zed, then open examples\basic.http and test Send.'
