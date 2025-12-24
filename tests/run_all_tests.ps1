# Comprehensive Test Suite for NeoDepends v0.0.15-pyfork (PowerShell)
# Tests all bug fixes from v0.0.14:
# 1. Windows Unicode crash fix (Method->Field instead of Method→Field)
# 2. Auto-select resolver (Python→stackgraphs, Java→depends)
# 3. Git Bash documentation
# 4. Output folder restructure (details/ subfolder)

$ErrorActionPreference = "Stop"

# Get script directory
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Split-Path -Parent $ScriptDir
Set-Location $RepoRoot

# Test output directory
$TestOutput = Join-Path $ScriptDir "test_output"
if (Test-Path $TestOutput) {
    Remove-Item -Recurse -Force $TestOutput
}
New-Item -ItemType Directory -Force -Path $TestOutput | Out-Null

# Counters
$script:TestsPassed = 0
$script:TestsFailed = 0
$script:TestsTotal = 0

# Helper functions
function Log-Test {
    param([string]$Message)
    Write-Host ""
    Write-Host "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" -ForegroundColor Blue
    Write-Host "TEST $($script:TestsTotal + 1): $Message" -ForegroundColor Blue
    Write-Host "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" -ForegroundColor Blue
}

function Log-Pass {
    param([string]$Message)
    Write-Host "✓ PASS: $Message" -ForegroundColor Green
    $script:TestsPassed++
    $script:TestsTotal++
}

function Log-Fail {
    param([string]$Message)
    Write-Host "✗ FAIL: $Message" -ForegroundColor Red
    $script:TestsFailed++
    $script:TestsTotal++
}

function Log-Info {
    param([string]$Message)
    Write-Host "ℹ $Message" -ForegroundColor Yellow
}

# Detect binary location
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

Write-Host ""
Write-Host "╔════════════════════════════════════════════════════════════════╗" -ForegroundColor Green
Write-Host "║  NeoDepends v0.0.15-pyfork Comprehensive Test Suite           ║" -ForegroundColor Green
Write-Host "╚════════════════════════════════════════════════════════════════╝" -ForegroundColor Green
Write-Host ""
Write-Host "Repository: $RepoRoot"
Write-Host "Binary: $NeodependsBin"
Write-Host "Test Output: $TestOutput"
Write-Host ""

# ============================================================================
# TEST 1: Unicode Fix - Check enhance_python_deps.py has no Unicode arrows
# ============================================================================
Log-Test "Unicode Fix - No → characters in enhance_python_deps.py"

$UnicodeLines = Select-String -Path "tools\enhance_python_deps.py" -Pattern '→' -AllMatches
$UnicodeCount = if ($UnicodeLines) { $UnicodeLines.Count } else { 0 }

if ($UnicodeCount -eq 0) {
    Log-Pass "No Unicode arrow characters (→) found in enhance_python_deps.py"
} else {
    Log-Fail "Found $UnicodeCount Unicode arrow characters (→) in enhance_python_deps.py"
    $UnicodeLines | ForEach-Object { Write-Host "  Line $($_.LineNumber): $($_.Line)" }
}

# ============================================================================
# TEST 2: Auto-Resolver - Check scripts have auto-selection logic
# ============================================================================
Log-Test "Auto-Resolver - Bash script has auto-selection"

if (Select-String -Path "run_dependency_analysis.sh" -Pattern "Auto-selected resolver: stackgraphs \(for Python\)" -Quiet) {
    Log-Pass "Bash script has auto-resolver for Python"
} else {
    Log-Fail "Bash script missing auto-resolver for Python"
}

if (Select-String -Path "run_dependency_analysis.sh" -Pattern "Auto-selected resolver: depends \(for Java\)" -Quiet) {
    Log-Pass "Bash script has auto-resolver for Java"
} else {
    Log-Fail "Bash script missing auto-resolver for Java"
}

Log-Test "Auto-Resolver - PowerShell script has auto-selection"

if (Select-String -Path "run_dependency_analysis.ps1" -Pattern "Auto-selected resolver: stackgraphs \(for Python\)" -Quiet) {
    Log-Pass "PowerShell script has auto-resolver for Python"
} else {
    Log-Fail "PowerShell script missing auto-resolver for Python"
}

if (Select-String -Path "run_dependency_analysis.ps1" -Pattern "Auto-selected resolver: depends \(for Java\)" -Quiet) {
    Log-Pass "PowerShell script has auto-resolver for Java"
} else {
    Log-Fail "PowerShell script missing auto-resolver for Java"
}

# ============================================================================
# TEST 3: Documentation - Check README has Cross-Platform Setup Instructions
# ============================================================================
Log-Test "Documentation - README has cross-platform setup instructions"

if (Select-String -Path "README.md" -Pattern "QuickStart Release Bundle: One-Command Setup & Analysis" -Quiet) {
    Log-Pass "README has QuickStart cross-platform setup section"
} else {
    Log-Fail "README missing QuickStart setup section"
}

if (Select-String -Path "README.md" -Pattern "python3 setup.py" -Quiet) {
    Log-Pass "README includes Python setup command"
} else {
    Log-Fail "README missing Python setup command"
}

if (Select-String -Path "README.md" -Pattern "python3 run_dependency_analysis.py" -Quiet) {
    Log-Pass "README includes Python analysis command"
} else {
    Log-Fail "README missing Python analysis command"
}

# ============================================================================
# TEST 4: Folder Structure - Run Python analysis and check details/ folder
# ============================================================================
Log-Test "Folder Structure - Python analysis creates details/ folder"

Log-Info "Running Python analysis on TOY example..."
$PythonTestDir = Join-Path $TestOutput "python_test"
$PythonLogFile = Join-Path $TestOutput "python_test.log"

& py -3 tools\neodepends_python_export.py `
  --neodepends-bin $NeodependsBin `
  --input examples\TrainTicketSystem_TOY_PYTHON_FIRST\tts `
  --output-dir $PythonTestDir `
  --resolver stackgraphs `
  --stackgraphs-python-mode ast `
  --dv8-hierarchy structured `
  --file-level-dv8 `
  --filter-architecture `
  --filter-stackgraphs-false-positives `
  > $PythonLogFile 2>&1

# Check details/ folder exists
if (Test-Path "$PythonTestDir\details") {
    Log-Pass "details/ folder created"
} else {
    Log-Fail "details/ folder NOT created"
}

# Check main files in root
if (Test-Path "$PythonTestDir\dependencies.stackgraphs_ast.db") {
    Log-Pass "Main DB in root: dependencies.stackgraphs_ast.db"
} else {
    Log-Fail "Main DB NOT in root"
}

if (Test-Path "$PythonTestDir\dependencies.stackgraphs_ast.filtered.dv8-dsm-v3.json") {
    Log-Pass "Main DV8 DSM in root: dependencies.stackgraphs_ast.filtered.dv8-dsm-v3.json"
} else {
    Log-Fail "Main DV8 DSM NOT in root"
}

# Check file-level DV8 in details/
if (Test-Path "$PythonTestDir\details\dependencies.stackgraphs_ast.file.dv8-dsm-v3.json") {
    Log-Pass "File-level DV8 in details/: dependencies.stackgraphs_ast.file.dv8-dsm-v3.json"
} else {
    Log-Fail "File-level DV8 NOT in details/"
}

# Check intermediate files in details/
if (Test-Path "$PythonTestDir\details\dv8_deps") {
    Log-Pass "Per-file DV8s in details\dv8_deps\"
} else {
    Log-Fail "Per-file DV8s NOT in details\dv8_deps\"
}

if (Test-Path "$PythonTestDir\details\per_file_dbs") {
    Log-Pass "Per-file DBs in details\per_file_dbs\"
} else {
    Log-Fail "Per-file DBs NOT in details\per_file_dbs\"
}

if (Test-Path "$PythonTestDir\details\run_summary.json") {
    Log-Pass "run_summary.json in details/"
} else {
    Log-Fail "run_summary.json NOT in details/"
}

# Check raw folders in details/
if (Test-Path "$PythonTestDir\details\raw") {
    Log-Pass "Raw output in details\raw\"
} else {
    Log-Fail "Raw output NOT in details\raw\"
}

if (Test-Path "$PythonTestDir\details\raw_filtered") {
    Log-Pass "Filtered raw output in details\raw_filtered\"
} else {
    Log-Fail "Filtered raw output NOT in details\raw_filtered\"
}

# ============================================================================
# TEST 5: Enhancement Script - Verify ASCII arrows in output
# ============================================================================
Log-Test "Enhancement Script - Output uses ASCII arrows (->)"

$LogContent = Get-Content $PythonLogFile -Raw

if ($LogContent -match "Method->Field") {
    Log-Pass "Enhancement script output uses ASCII arrows (Method->Field)"
} else {
    Log-Fail "Enhancement script output doesn't use ASCII arrows"
}

# Should NOT have Unicode arrows
if ($LogContent -match "Method→Field") {
    Log-Fail "Found Unicode arrows (→) in enhancement script output!"
} else {
    Log-Pass "No Unicode arrows (→) in enhancement script output"
}

# ============================================================================
# TEST 6: QuickStart Examples - Run all 4 examples
# ============================================================================
Log-Test "QuickStart Examples - All 4 examples run successfully"

Log-Info "Running QuickStart examples..."
$QuickStartLog = Join-Path $TestOutput "quickstart.log"
& .\QuickStart_dependency_analysis_examples.ps1 > $QuickStartLog 2>&1

# Check Python TOY 1
if (Test-Path "RESULTS_QuickStart_Examples\python_toy_first\dependencies.stackgraphs_ast.filtered.dv8-dsm-v3.json") {
    Log-Pass "Python TOY 1 - DV8 file created"
} else {
    Log-Fail "Python TOY 1 - DV8 file NOT created"
}

if (Test-Path "RESULTS_QuickStart_Examples\python_toy_first\details") {
    Log-Pass "Python TOY 1 - details/ folder created"
} else {
    Log-Fail "Python TOY 1 - details/ folder NOT created"
}

# Check Python TOY 2
if (Test-Path "RESULTS_QuickStart_Examples\python_toy_second\dependencies.stackgraphs_ast.filtered.dv8-dsm-v3.json") {
    Log-Pass "Python TOY 2 - DV8 file created"
} else {
    Log-Fail "Python TOY 2 - DV8 file NOT created"
}

if (Test-Path "RESULTS_QuickStart_Examples\python_toy_second\details") {
    Log-Pass "Python TOY 2 - details/ folder created"
} else {
    Log-Fail "Python TOY 2 - details/ folder NOT created"
}

# Check Java TOY 1
if (Test-Path "RESULTS_QuickStart_Examples\java_toy_first\dependencies.dv8-dsm-v3.json") {
    Log-Pass "Java TOY 1 - DV8 file created"
} else {
    Log-Fail "Java TOY 1 - DV8 file NOT created"
}

# Check Java TOY 2
if (Test-Path "RESULTS_QuickStart_Examples\java_toy_second\dependencies.dv8-dsm-v3.json") {
    Log-Pass "Java TOY 2 - DV8 file created"
} else {
    Log-Fail "Java TOY 2 - DV8 file NOT created"
}

# ============================================================================
# TEST 7: JSON Validation - Verify all generated JSON files are valid
# ============================================================================
Log-Test "JSON Validation - All generated DV8 files are valid JSON"

$JsonFiles = Get-ChildItem -Path $PythonTestDir -Recurse -Include "*.json"
$JsonCount = $JsonFiles.Count
$JsonValid = 0

foreach ($JsonFile in $JsonFiles) {
    try {
        $null = Get-Content $JsonFile.FullName -Raw | ConvertFrom-Json
        $JsonValid++
    } catch {
        Log-Fail "Invalid JSON: $($JsonFile.FullName)"
    }
}

if ($JsonCount -eq $JsonValid) {
    Log-Pass "All $JsonCount JSON files are valid"
} else {
    Log-Fail "$($JsonCount - $JsonValid) out of $JsonCount JSON files are invalid"
}

# ============================================================================
# FINAL SUMMARY
# ============================================================================
Write-Host ""
Write-Host "╔════════════════════════════════════════════════════════════════╗" -ForegroundColor Green
Write-Host "║                      TEST SUMMARY                              ║" -ForegroundColor Green
Write-Host "╚════════════════════════════════════════════════════════════════╝" -ForegroundColor Green
Write-Host ""
Write-Host "Total Tests: $script:TestsTotal"
Write-Host "Passed: $script:TestsPassed" -ForegroundColor Green
Write-Host "Failed: $script:TestsFailed" -ForegroundColor Red
Write-Host ""

if ($script:TestsFailed -eq 0) {
    Write-Host "╔════════════════════════════════════════════════════════════════╗" -ForegroundColor Green
    Write-Host "║  ✓ ALL TESTS PASSED! Ready for v0.0.15-pyfork release        ║" -ForegroundColor Green
    Write-Host "╚════════════════════════════════════════════════════════════════╝" -ForegroundColor Green
    exit 0
} else {
    Write-Host "╔════════════════════════════════════════════════════════════════╗" -ForegroundColor Red
    Write-Host "║  ✗ SOME TESTS FAILED! Please review and fix issues            ║" -ForegroundColor Red
    Write-Host "╚════════════════════════════════════════════════════════════════╝" -ForegroundColor Red
    Write-Host ""
    Write-Host "Test logs available in: $TestOutput"
    exit 1
}
