# QuickStart script to run NeoDepends on all example projects (PowerShell)
# This script demonstrates how to use NeoDepends on both Python and Java projects
#
# Usage (from neodepends directory):
#   .\QuickStart_dependency_analysis_examples.ps1

$ErrorActionPreference = "Stop"

# Get script directory
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $ScriptDir

# Output directory
$OutputRoot = Join-Path $ScriptDir "RESULTS_QuickStart_Examples"

Write-Host "=== NeoDepends QuickStart Examples ==="
Write-Host "Output directory: $OutputRoot"
Write-Host ""

# Clean and create output directory
if (Test-Path $OutputRoot) {
    Remove-Item -Recurse -Force $OutputRoot
}
New-Item -ItemType Directory -Force -Path $OutputRoot | Out-Null

# Detect if we're in a bundle or source build
if (Test-Path ".\neodepends.exe") {
    $NeodependsBin = ".\neodepends.exe"
} elseif (Test-Path ".\target\release\neodepends.exe") {
    $NeodependsBin = ".\target\release\neodepends.exe"
} else {
    Write-Host "ERROR: neodepends binary not found!" -ForegroundColor Red
    Write-Host "Please either:"
    Write-Host "  - Use a release bundle (neodepends.exe in root directory)"
    Write-Host "  - Build from source: cargo build --release"
    exit 1
}

Write-Host "Using NeoDepends binary: $NeodependsBin"
Write-Host ""

# ============================================================================
# PYTHON EXAMPLES (using StackGraphs AST resolver)
# ============================================================================

$PythonFlags = @(
    "--resolver", "stackgraphs",
    "--stackgraphs-python-mode", "ast",
    "--dv8-hierarchy", "structured",
    "--filter-architecture",
    "--filter-stackgraphs-false-positives"
)

Write-Host "=== 1/4: Python Example - TrainTicketSystem TOY 1 ==="
& py -3 tools\neodepends_python_export.py `
    --neodepends-bin $NeodependsBin `
    --input "examples\TrainTicketSystem_TOY_PYTHON_FIRST\tts" `
    --output-dir "$OutputRoot\python_toy_first" `
    @PythonFlags
Write-Host "✓ Python TOY 1 complete"
Write-Host ""

Write-Host "=== 2/4: Python Example - TrainTicketSystem TOY 2 ==="
& py -3 tools\neodepends_python_export.py `
    --neodepends-bin $NeodependsBin `
    --input "examples\TrainTicketSystem_TOY_PYTHON_SECOND\tts" `
    --output-dir "$OutputRoot\python_toy_second" `
    @PythonFlags
Write-Host "✓ Python TOY 2 complete"
Write-Host ""

# ============================================================================
# JAVA EXAMPLES (using Depends resolver)
# ============================================================================

Write-Host "=== 3/4: Java Example - TrainTicketSystem TOY 1 ==="
New-Item -ItemType Directory -Force -Path "$OutputRoot\java_toy_first" | Out-Null

& $NeodependsBin `
    --input "examples\TrainTicketSystem_TOY_JAVA_FIRST\src" `
    --output "$OutputRoot\java_toy_first\dependencies.db" `
    --format sqlite `
    --resources entities,deps,contents `
    --langs java `
    --depends `
    --depends-jar .\artifacts\depends.jar `
    --force

& py -3 tools\export_dv8_from_neodepends_db.py `
    --db "$OutputRoot\java_toy_first\dependencies.db" `
    --out "$OutputRoot\java_toy_first\dependencies.dv8-dsm-v3.json" `
    --name "Java TOY 1 (TrainTicketSystem)"

Write-Host "✓ Java TOY 1 complete"
Write-Host ""

Write-Host "=== 4/4: Java Example - TrainTicketSystem TOY 2 ==="
New-Item -ItemType Directory -Force -Path "$OutputRoot\java_toy_second" | Out-Null

& $NeodependsBin `
    --input "examples\TrainTicketSystem_TOY_JAVA_SECOND\src" `
    --output "$OutputRoot\java_toy_second\dependencies.db" `
    --format sqlite `
    --resources entities,deps,contents `
    --langs java `
    --depends `
    --depends-jar .\artifacts\depends.jar `
    --force

& py -3 tools\export_dv8_from_neodepends_db.py `
    --db "$OutputRoot\java_toy_second\dependencies.db" `
    --out "$OutputRoot\java_toy_second\dependencies.dv8-dsm-v3.json" `
    --name "Java TOY 2 (TrainTicketSystem)"

Write-Host "✓ Java TOY 2 complete"
Write-Host ""

# ============================================================================
# Summary
# ============================================================================

Write-Host "=== QuickStart Complete! ==="
Write-Host ""
Write-Host "Results saved to: $OutputRoot"
Write-Host ""
Write-Host "Python DV8 DSM files (open in DV8 Explorer):"
Write-Host "  - $OutputRoot\python_toy_first\dependencies.stackgraphs_ast.filtered.dv8-dsm-v3.json"
Write-Host "  - $OutputRoot\python_toy_second\dependencies.stackgraphs_ast.filtered.dv8-dsm-v3.json"
Write-Host ""
Write-Host "Java DV8 DSM files (open in DV8 Explorer):"
Write-Host "  - $OutputRoot\java_toy_first\dependencies.dv8-dsm-v3.json"
Write-Host "  - $OutputRoot\java_toy_second\dependencies.dv8-dsm-v3.json"
Write-Host ""
Write-Host "Next steps:"
Write-Host "  1. Open any .dv8-dsm-v3.json file in DV8 Explorer for visualization"
Write-Host "  2. See README.md for how to run on your own projects"
Write-Host ""
