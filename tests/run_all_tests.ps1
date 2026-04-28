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

# Resolve Python executable
$PythonExe = (Get-Command python -ErrorAction SilentlyContinue)
if (-not $PythonExe) {
    Write-Host "ERROR: python not found in PATH." -ForegroundColor Red
    Write-Host "Install Python or ensure actions/setup-python is configured."
    exit 1
}
$PythonExe = $PythonExe.Source

Write-Host ""
Write-Host "╔════════════════════════════════════════════════════════════════╗" -ForegroundColor Green
Write-Host "║  NeoDepends Python Extension Release Test Suite               ║" -ForegroundColor Green
Write-Host "╚════════════════════════════════════════════════════════════════╝" -ForegroundColor Green
Write-Host ""
Write-Host "Repository: $RepoRoot"
Write-Host "Binary: $NeodependsBin"
Write-Host "Test Output: $TestOutput"
Write-Host ""

# Auto-detect or auto-clone TOY repo for comparisons
$ToyRoot = $env:TOY_ROOT
if (-not $ToyRoot) {
    $Candidates = @(
        (Join-Path $RepoRoot "..\\..\\..\\..\\000_TOY_EXAMPLES\\ARCH_ANALYSIS_TRAINTICKET_TOY_EXAMPLES_MULTILANG"),
        (Join-Path $RepoRoot "..\\..\\000_TOY_EXAMPLES\\ARCH_ANALYSIS_TRAINTICKET_TOY_EXAMPLES_MULTILANG"),
        (Join-Path $RepoRoot "..\\000_TOY_EXAMPLES\\ARCH_ANALYSIS_TRAINTICKET_TOY_EXAMPLES_MULTILANG")
    )
    foreach ($c in $Candidates) {
        if (Test-Path $c) {
            $ToyRoot = (Resolve-Path $c).Path
            break
        }
    }
}
if (-not $ToyRoot) {
    $ToyRepoUrl = $env:TOY_REPO_URL
    if (-not $ToyRepoUrl) {
        $ToyRepoUrl = "https://github.com/FreeworkEarth/ARCH_ANALYSIS_TRAINTICKET_TOY_EXAMPLES_MULTILANG.git"
    }
    $ToyCloneDir = Join-Path $env:TEMP "neodepends_toy"
    if (Get-Command git -ErrorAction SilentlyContinue) {
        if (Test-Path (Join-Path $ToyCloneDir ".git")) {
            git -C $ToyCloneDir fetch --depth 1 origin main | Out-Null
            git -C $ToyCloneDir reset --hard origin/main | Out-Null
        } else {
            if (Test-Path $ToyCloneDir) { Remove-Item -Recurse -Force $ToyCloneDir }
            git clone --depth 1 --branch main $ToyRepoUrl $ToyCloneDir | Out-Null
        }
    }
    if (Test-Path (Join-Path $ToyCloneDir "python\\first_godclass_antipattern")) {
        $ToyRoot = (Resolve-Path $ToyCloneDir).Path
    }
}
if ($ToyRoot -and (Test-Path (Join-Path $ToyRoot "python\\first_godclass_antipattern"))) {
    $env:TOY_ROOT = $ToyRoot
    Log-Info "Using TOY_ROOT=$ToyRoot"
} else {
    Log-Info "TOY_ROOT not set; toy handcount comparisons will be skipped (survey/moviepy/large_single_file still run)"
}

# Optional: run example comparison (toy + survey + moviepy if available)
if (Test-Path "tools\\run_handcount_regression.py") {
    $HandcountOut = Join-Path $TestOutput "handcount_regression"
    $ToyArgs = @()
    if ($env:TOY_ROOT -and (Test-Path $env:TOY_ROOT)) {
        $ToyArgs = @("--toy-root", $env:TOY_ROOT)
    }
    Log-Test "Example Comparison (diffs always generated)"
    & $PythonExe "tools\\run_handcount_regression.py" `
        --neodepends-bin $NeodependsBin `
        --depends-jar "artifacts\\depends.jar" `
        --output-dir $HandcountOut `
        @ToyArgs
    if ($LASTEXITCODE -eq 0) {
        Log-Pass "Handcount regression completed"
    } else {
        Log-Fail "Handcount regression failed"
    }
}

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

Log-Info "Skipped on production (client README differs)"

# ============================================================================
# TEST 4: Setup Script - Verify setup.py exists and works
# ============================================================================
Log-Test "Setup Script - Verify setup.py exists and is executable"

Log-Info "Skipped on production (setup.py not shipped in release repo)"

# ============================================================================
# TEST 5: Folder Structure - Run Python analysis and check data/ folder
# ============================================================================
Log-Test "Folder Structure - Python analysis creates data/ folder"

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

# Check data/ folder exists
if (Test-Path "$PythonTestDir\data") {
    Log-Pass "data/ folder created"
} else {
    Log-Fail "data/ folder NOT created"
}

# Check main files in root
if (Test-Path "$PythonTestDir\data\dependencies.stackgraphs_ast.db") {
    Log-Pass "Main DB in root: dependencies.stackgraphs_ast.db"
} else {
    Log-Fail "Main DB NOT in root"
}

if (Test-Path "$PythonTestDir\analysis-result.json") {
    Log-Pass "Main DV8 DSM in root: analysis-result.json"
} else {
    Log-Fail "Main DV8 DSM NOT in root"
}

# Check file-level DV8 in data/
if (Test-Path "$PythonTestDir\data\dv8_deps") {
    Log-Pass "Per-file DV8s in data\\dv8_deps\\"
} else {
    Log-Fail "Per-file DV8s NOT in data\\dv8_deps\\"
}

# Check intermediate files in data/
if (Test-Path "$PythonTestDir\data\per_file_dbs") {
    Log-Pass "Per-file DBs in data\\per_file_dbs\\"
} else {
    Log-Fail "Per-file DBs NOT in data\\per_file_dbs\\"
}

if (Test-Path "$PythonTestDir\data\run_summary.json") {
    Log-Pass "run_summary.json in data/"
} else {
    Log-Fail "run_summary.json NOT in data/"
}

# Check raw folders in data/
if (Test-Path "$PythonTestDir\data\raw") {
    Log-Pass "Raw output in data\\raw\\"
} else {
    Log-Fail "Raw output NOT in data\\raw\\"
}

if (Test-Path "$PythonTestDir\data\raw_filtered") {
    Log-Pass "Filtered raw output in data\\raw_filtered\\"
} else {
    Log-Fail "Filtered raw output NOT in data\\raw_filtered\\"
}

# ============================================================================
# TEST 5: Enhancement Script - Verify ASCII arrows in output
# ============================================================================
Log-Test "Enhancement Script - Output uses ASCII arrows (->)"

# Enhancement output may go to stdout (main) or dev_log/dev_log.txt (production)
$LogContent = if (Test-Path $PythonLogFile) { Get-Content $PythonLogFile -Raw -ErrorAction SilentlyContinue } else { "" }
$DevLogFile = Join-Path $TestOutput "python_test\data\dev_log\dev_log.txt"
$DevLogContent = if (Test-Path $DevLogFile) { Get-Content $DevLogFile -Raw -ErrorAction SilentlyContinue } else { "" }
$CombinedLog = "$LogContent`n$DevLogContent"

if ($CombinedLog -match "Method->Field") {
    Log-Pass "Enhancement script output uses ASCII arrows (Method->Field)"
} else {
    Log-Fail "Enhancement script output doesn't use ASCII arrows"
}

# Should NOT have Unicode arrows
if ($CombinedLog -match "Method→Field") {
    Log-Fail "Found Unicode arrows (→) in enhancement script output!"
} else {
    Log-Pass "No Unicode arrows (→) in enhancement script output"
}

# ============================================================================
# TEST 6: Single-File Analysis - VideoClip.py (43k lines)
# ============================================================================
Log-Test "Single-File Analysis - VideoClip.py (large single file)"

Log-Info "Analyzing VideoClip.py (43k lines)..."
$VideoClipPath = (Resolve-Path "examples\Large_Single_File_PYTHON_videoclip\VideoClip.py").Path
$VideoClipTestDir = Join-Path $TestOutput "videoclip_test"
$VideoClipLogFile = Join-Path $TestOutput "videoclip_test.log"

& py -3 tools\neodepends_python_export.py `
  --neodepends-bin $NeodependsBin `
  --input $VideoClipPath `
  --output-dir $VideoClipTestDir `
  --resolver stackgraphs `
  --stackgraphs-python-mode ast `
  --dv8-hierarchy structured `
  --file-level-dv8 `
  --filter-architecture `
  --filter-stackgraphs-false-positives `
  > $VideoClipLogFile 2>&1

if (Test-Path "$VideoClipTestDir\analysis-result.json") {
    Log-Pass "Single-file analysis successful - DV8 file created"
    $VideoClipSize = (Get-Item "$VideoClipTestDir\analysis-result.json").Length
    Log-Info "Output size: $VideoClipSize bytes"

    # Count dependencies in DB and JSON (using Python for cross-platform SQLite access)
    $VideoClipDbDeps = & py -3 -c "import sqlite3; conn=sqlite3.connect('$VideoClipTestDir/dependencies.stackgraphs_ast.db'.replace('\\', '/')); print(conn.execute('SELECT COUNT(*) FROM deps').fetchone()[0]); conn.close()" 2>$null
    if (-not $VideoClipDbDeps) { $VideoClipDbDeps = 0 }

    $VideoClipJsonCells = & py -3 -c "import json; data=json.load(open('$VideoClipTestDir/analysis-result.json'.replace('\\', '/'))); print(len(data.get('cells', [])))" 2>$null
    if (-not $VideoClipJsonCells) { $VideoClipJsonCells = 0 }

    $VideoClipJsonVars = & py -3 -c "import json; data=json.load(open('$VideoClipTestDir/analysis-result.json'.replace('\\', '/'))); print(len(data.get('variables', [])))" 2>$null
    if (-not $VideoClipJsonVars) { $VideoClipJsonVars = 0 }

    Log-Info "DB deps: $VideoClipDbDeps, JSON cells: $VideoClipJsonCells, JSON variables: $VideoClipJsonVars"
} else {
    Log-Fail "Single-file analysis FAILED - no DV8 file"
}

if (Test-Path "$VideoClipTestDir\data") {
    Log-Pass "Single-file analysis created data/ folder"
} else {
    Log-Fail "Single-file analysis did NOT create data/ folder"
}

# Check if enhancement completed
$VideoClipLogContent = Get-Content $VideoClipLogFile -Raw
if ($VideoClipLogContent -match "Method->Field dependencies created") {
    Log-Info "Single-file enhancement completed"
}

# ============================================================================
# TEST 7: Real Project Analysis - Moviepy
# ============================================================================
Log-Test "Real Project Analysis - Moviepy"

$MoviepyPath = "examples\examples_testing\Py\moviepy example\moviepy"
if (Test-Path $MoviepyPath) {
    Log-Info "Analyzing Moviepy project..."
    $MoviepyTestDir = Join-Path $TestOutput "moviepy_test"
    $MoviepyLogFile = Join-Path $TestOutput "moviepy_test.log"

    & py -3 tools\neodepends_python_export.py `
      --neodepends-bin $NeodependsBin `
      --input $MoviepyPath `
      --output-dir $MoviepyTestDir `
      --resolver stackgraphs `
      --stackgraphs-python-mode ast `
      --dv8-hierarchy structured `
      --file-level-dv8 `
      --filter-architecture `
      --filter-stackgraphs-false-positives `
      > $MoviepyLogFile 2>&1

    if (Test-Path "$MoviepyTestDir\analysis-result.json") {
        Log-Pass "Moviepy analysis successful"
        $MoviepySize = (Get-Item "$MoviepyTestDir\analysis-result.json").Length
        Log-Info "Output size: $MoviepySize bytes"

        # Extract metrics from log
        $MoviepyLogContent = Get-Content $MoviepyLogFile -Raw
        if ($MoviepyLogContent -match "(\d+) Method->Field dependencies created") {
            $MoviepyMethodField = $Matches[1]
        } else {
            $MoviepyMethodField = 0
        }
        if ($MoviepyLogContent -match "(\d+) fields now siblings with methods") {
            $MoviepyFieldsMoved = $Matches[1]
        } else {
            $MoviepyFieldsMoved = 0
        }

        # Count dependencies in DB and JSON (using Python for cross-platform SQLite access)
        $MoviepyDbDeps = & py -3 -c "import sqlite3; conn=sqlite3.connect('$MoviepyTestDir/dependencies.stackgraphs_ast.db'.replace('\\', '/')); print(conn.execute('SELECT COUNT(*) FROM deps').fetchone()[0]); conn.close()" 2>$null
        if (-not $MoviepyDbDeps) { $MoviepyDbDeps = 0 }

        $MoviepyJsonCells = & py -3 -c "import json; data=json.load(open('$MoviepyTestDir/analysis-result.json'.replace('\\', '/'))); print(len(data.get('cells', [])))" 2>$null
        if (-not $MoviepyJsonCells) { $MoviepyJsonCells = 0 }

        $MoviepyJsonVars = & py -3 -c "import json; data=json.load(open('$MoviepyTestDir/analysis-result.json'.replace('\\', '/'))); print(len(data.get('variables', [])))" 2>$null
        if (-not $MoviepyJsonVars) { $MoviepyJsonVars = 0 }

        Log-Info "Method->Field deps: $MoviepyMethodField, Fields moved: $MoviepyFieldsMoved"
        Log-Info "DB deps: $MoviepyDbDeps, JSON cells: $MoviepyJsonCells, JSON variables: $MoviepyJsonVars"
    } else {
        Log-Fail "Moviepy analysis FAILED"
    }

    if (Test-Path "$MoviepyTestDir\data") {
        Log-Pass "Moviepy created data/ folder"
    } else {
        Log-Fail "Moviepy did NOT create data/ folder"
    }
} else {
    Log-Info "Moviepy example not found, skipping..."
}

# ============================================================================
# TEST 8: Real Project Analysis - Survey
# ============================================================================
Log-Test "Real Project Analysis - Survey"

$SurveyPath = "examples\examples_testing\Py\survey example\Survey3"
if (Test-Path $SurveyPath) {
    Log-Info "Analyzing Survey project..."
    $SurveyTestDir = Join-Path $TestOutput "survey_test"
    $SurveyLogFile = Join-Path $TestOutput "survey_test.log"

    & py -3 tools\neodepends_python_export.py `
      --neodepends-bin $NeodependsBin `
      --input $SurveyPath `
      --output-dir $SurveyTestDir `
      --resolver stackgraphs `
      --stackgraphs-python-mode ast `
      --dv8-hierarchy structured `
      --file-level-dv8 `
      --filter-architecture `
      --filter-stackgraphs-false-positives `
      > $SurveyLogFile 2>&1

    if (Test-Path "$SurveyTestDir\analysis-result.json") {
        Log-Pass "Survey analysis successful"
        $SurveySize = (Get-Item "$SurveyTestDir\analysis-result.json").Length
        Log-Info "Output size: $SurveySize bytes"

        # Extract metrics from log
        $SurveyLogContent = Get-Content $SurveyLogFile -Raw
        if ($SurveyLogContent -match "(\d+) Method->Field dependencies created") {
            $SurveyMethodField = $Matches[1]
        } else {
            $SurveyMethodField = 0
        }
        if ($SurveyLogContent -match "(\d+) fields now siblings with methods") {
            $SurveyFieldsMoved = $Matches[1]
        } else {
            $SurveyFieldsMoved = 0
        }

        # Count dependencies in DB and JSON (using Python for cross-platform SQLite access)
        $SurveyDbDeps = & py -3 -c "import sqlite3; conn=sqlite3.connect('$SurveyTestDir/dependencies.stackgraphs_ast.db'.replace('\\', '/')); print(conn.execute('SELECT COUNT(*) FROM deps').fetchone()[0]); conn.close()" 2>$null
        if (-not $SurveyDbDeps) { $SurveyDbDeps = 0 }

        $SurveyJsonCells = & py -3 -c "import json; data=json.load(open('$SurveyTestDir/analysis-result.json'.replace('\\', '/'))); print(len(data.get('cells', [])))" 2>$null
        if (-not $SurveyJsonCells) { $SurveyJsonCells = 0 }

        $SurveyJsonVars = & py -3 -c "import json; data=json.load(open('$SurveyTestDir/analysis-result.json'.replace('\\', '/'))); print(len(data.get('variables', [])))" 2>$null
        if (-not $SurveyJsonVars) { $SurveyJsonVars = 0 }

        Log-Info "Method->Field deps: $SurveyMethodField, Fields moved: $SurveyFieldsMoved"
        Log-Info "DB deps: $SurveyDbDeps, JSON cells: $SurveyJsonCells, JSON variables: $SurveyJsonVars"
    } else {
        Log-Fail "Survey analysis FAILED"
    }

    if (Test-Path "$SurveyTestDir\data") {
        Log-Pass "Survey created data/ folder"
    } else {
        Log-Fail "Survey did NOT create data/ folder"
    }
} else {
    Log-Info "Survey example not found, skipping..."
}

# ============================================================================
# TEST 9: QuickStart Examples - Run all 4 examples
# ============================================================================
Log-Test "QuickStart Examples - All 4 examples run successfully"

Log-Info "Running QuickStart examples..."
$QuickStartLog = Join-Path $TestOutput "quickstart.log"
& .\QuickStart_dependency_analysis_examples.ps1 > $QuickStartLog 2>&1

# Check Python TOY 1
if (Test-Path "RESULTS_QuickStart_Examples\python_toy_first\analysis-result.json") {
    Log-Pass "Python TOY 1 - DV8 file created"
} else {
    Log-Fail "Python TOY 1 - DV8 file NOT created"
}

if (Test-Path "RESULTS_QuickStart_Examples\python_toy_first\data") {
    Log-Pass "Python TOY 1 - data/ folder created"
} else {
    Log-Fail "Python TOY 1 - data/ folder NOT created"
}

# Check Python TOY 2
if (Test-Path "RESULTS_QuickStart_Examples\python_toy_second\analysis-result.json") {
    Log-Pass "Python TOY 2 - DV8 file created"
} else {
    Log-Fail "Python TOY 2 - DV8 file NOT created"
}

if (Test-Path "RESULTS_QuickStart_Examples\python_toy_second\data") {
    Log-Pass "Python TOY 2 - data/ folder created"
} else {
    Log-Fail "Python TOY 2 - data/ folder NOT created"
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
# TEST 10: JSON Validation - Verify all generated JSON files are valid
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
