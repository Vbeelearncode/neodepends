# NeoDepends Dependency Analysis Runner (PowerShell)
# This script prompts for inputs and runs the dependency analysis pipeline

$ErrorActionPreference = "Stop"

# Get script directory to find tools and neodepends binary
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $ScriptDir

# Prompt for neodepends binary path (with default)
$DefaultNeodepends = ".\neodepends.exe"
$NeodependsBin = Read-Host "Enter neodepends binary path [default: $DefaultNeodepends]"
$NeodependsBin = $NeodependsBin.Trim()
if ([string]::IsNullOrWhiteSpace($NeodependsBin)) {
    $NeodependsBin = $DefaultNeodepends
}

# Validate neodepends binary exists
if (-not (Test-Path $NeodependsBin)) {
    Write-Host "Error: NeoDepends binary not found at: $NeodependsBin" -ForegroundColor Red
    exit 1
}

# Prompt for input repository
$InputRepo = Read-Host "Enter input repository path"
if (-not (Test-Path $InputRepo)) {
    Write-Host "Error: Input repository path does not exist: $InputRepo" -ForegroundColor Red
    exit 1
}

# Prompt for output location
$OutputDir = Read-Host "Enter output directory path"
$OutputDir = $OutputDir.Trim()
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
    Write-Host "Error: Output directory cannot be empty" -ForegroundColor Red
    exit 1
}

# Create output directory if it doesn't exist
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

# Prompt for language
$Language = Read-Host "Enter language (python or java)"
$Language = $Language.ToLower().Trim()
if ($Language -ne "python" -and $Language -ne "java") {
    Write-Host "Error: Language must be 'python' or 'java'" -ForegroundColor Red
    exit 1
}

# Prompt for model/resolver (accept shortcuts: d/D/depends or s/S/stackgraphs)
$Model = Read-Host "Enter model (d/D/depends or s/S/stackgraphs)"
$Model = $Model.ToLower().Trim()

# Normalize shortcuts to full names
switch ($Model) {
    { $_ -in @("d", "depends") } {
        $Model = "depends"
    }
    { $_ -in @("s", "stackgraphs") } {
        $Model = "stackgraphs"
    }
    default {
        Write-Host "Error: Model must be 'd', 'depends', 's', or 'stackgraphs'" -ForegroundColor Red
        exit 1
    }
}

# Build the command arguments
$Args = @(
    "tools\neodepends_python_export.py",
    "--neodepends-bin", $NeodependsBin,
    "--input", $InputRepo,
    "--output-dir", $OutputDir,
    "--langs", $Language,
    "--resolver", $Model,
    "--dv8-hierarchy", "structured",
    "--filter-architecture"
)

# For Python with stackgraphs, add stackgraphs-specific flags
if ($Language -eq "python" -and $Model -eq "stackgraphs") {
    $Args += "--stackgraphs-python-mode", "ast"
    $Args += "--filter-stackgraphs-false-positives"
}

Write-Host ""
Write-Host "Running dependency analysis with the following settings:"
Write-Host "  NeoDepends binary: $NeodependsBin"
Write-Host "  Input repository: $InputRepo"
Write-Host "  Output directory: $OutputDir"
Write-Host "  Language: $Language"
Write-Host "  Model/Resolver: $Model"
Write-Host ""
Write-Host "Command: py -3 $($Args -join ' ')"
Write-Host ""

# Execute the command
& py -3 $Args

# Determine the output filename based on resolver
if ($Model -eq "depends") {
    $ResolverName = "depends"
} elseif ($Model -eq "stackgraphs" -and $Language -eq "python") {
    $ResolverName = "stackgraphs_ast"
} else {
    $ResolverName = "stackgraphs"
}
$OutputFile = Join-Path $OutputDir "dependencies.$ResolverName.filtered.dv8-dsm-v3.json"

Write-Host ""
Write-Host "================================================================================"
Write-Host "Dependency analysis complete!"
Write-Host ""
Write-Host "Results saved to: $OutputDir"
Write-Host ""
Write-Host "To visualize results in DV8 Explorer, open:"
Write-Host "  $OutputFile"
Write-Host "================================================================================"
