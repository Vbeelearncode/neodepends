#!/bin/bash
set -euo pipefail

# QuickStart script to run NeoDepends on all example projects
# This script demonstrates how to use NeoDepends on both Python and Java projects
#
# Usage (from neodepends directory):
#   chmod +x QuickStart_dependency_analysis_examples.sh
#   ./QuickStart_dependency_analysis_examples.sh

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Output directory
OUTPUT_ROOT="${SCRIPT_DIR}/RESULTS_QuickStart_Examples"

echo "=== NeoDepends QuickStart Examples ==="
echo "Output directory: ${OUTPUT_ROOT}"
echo ""

# Clean and create output directory
rm -rf "${OUTPUT_ROOT}"
mkdir -p "${OUTPUT_ROOT}"

# Detect if we're in a bundle or source build
if [ -f "./neodepends" ]; then
    NEODEPENDS_BIN="./neodepends"
elif [ -f "./target/release/neodepends" ]; then
    NEODEPENDS_BIN="./target/release/neodepends"
else
    echo "ERROR: neodepends binary not found!"
    echo "Please either:"
    echo "  - Use a release bundle (neodepends in root directory)"
    echo "  - Build from source: cargo build --release"
    exit 1
fi

echo "Using NeoDepends binary: ${NEODEPENDS_BIN}"
echo ""

# ============================================================================
# PYTHON EXAMPLES (using StackGraphs AST resolver)
# ============================================================================

PYTHON_FLAGS=(
    --resolver stackgraphs
    --stackgraphs-python-mode ast
    --dv8-hierarchy structured
    --filter-architecture
    --filter-stackgraphs-false-positives
)

echo "=== 1/4: Python Example - TrainTicketSystem TOY 1 ==="
python3 tools/neodepends_python_export.py \
    --neodepends-bin "${NEODEPENDS_BIN}" \
    --input "examples/TrainTicketSystem_TOY_PYTHON_FIRST/tts" \
    --output-dir "${OUTPUT_ROOT}/python_toy_first" \
    "${PYTHON_FLAGS[@]}"
echo "✓ Python TOY 1 complete"
echo ""

echo "=== 2/4: Python Example - TrainTicketSystem TOY 2 ==="
python3 tools/neodepends_python_export.py \
    --neodepends-bin "${NEODEPENDS_BIN}" \
    --input "examples/TrainTicketSystem_TOY_PYTHON_SECOND/tts" \
    --output-dir "${OUTPUT_ROOT}/python_toy_second" \
    "${PYTHON_FLAGS[@]}"
echo "✓ Python TOY 2 complete"
echo ""

# ============================================================================
# JAVA EXAMPLES (using Depends resolver)
# ============================================================================

echo "=== 3/4: Java Example - TrainTicketSystem TOY 1 ==="
mkdir -p "${OUTPUT_ROOT}/java_toy_first"

"${NEODEPENDS_BIN}" \
    --input "examples/TrainTicketSystem_TOY_JAVA_FIRST/src" \
    --output "${OUTPUT_ROOT}/java_toy_first/dependencies.db" \
    --format sqlite \
    --resources entities,deps,contents \
    --langs java \
    --depends \
    --depends-jar ./artifacts/depends.jar \
    --force

python3 tools/export_dv8_from_neodepends_db.py \
    --db "${OUTPUT_ROOT}/java_toy_first/dependencies.db" \
    --out "${OUTPUT_ROOT}/java_toy_first/dependencies.dv8-dsm-v3.json" \
    --name "Java TOY 1 (TrainTicketSystem)"

echo "✓ Java TOY 1 complete"
echo ""

echo "=== 4/4: Java Example - TrainTicketSystem TOY 2 ==="
mkdir -p "${OUTPUT_ROOT}/java_toy_second"

"${NEODEPENDS_BIN}" \
    --input "examples/TrainTicketSystem_TOY_JAVA_SECOND/src" \
    --output "${OUTPUT_ROOT}/java_toy_second/dependencies.db" \
    --format sqlite \
    --resources entities,deps,contents \
    --langs java \
    --depends \
    --depends-jar ./artifacts/depends.jar \
    --force

python3 tools/export_dv8_from_neodepends_db.py \
    --db "${OUTPUT_ROOT}/java_toy_second/dependencies.db" \
    --out "${OUTPUT_ROOT}/java_toy_second/dependencies.dv8-dsm-v3.json" \
    --name "Java TOY 2 (TrainTicketSystem)"

echo "✓ Java TOY 2 complete"
echo ""

# ============================================================================
# Summary
# ============================================================================

echo "=== QuickStart Complete! ==="
echo ""
echo "Results saved to: ${OUTPUT_ROOT}"
echo ""
echo "Python DV8 DSM files (open in DV8 Explorer):"
echo "  - ${OUTPUT_ROOT}/python_toy_first/dependencies.stackgraphs_ast.filtered.dv8-dsm-v3.json"
echo "  - ${OUTPUT_ROOT}/python_toy_second/dependencies.stackgraphs_ast.filtered.dv8-dsm-v3.json"
echo ""
echo "Java DV8 DSM files (open in DV8 Explorer):"
echo "  - ${OUTPUT_ROOT}/java_toy_first/dependencies.dv8-dsm-v3.json"
echo "  - ${OUTPUT_ROOT}/java_toy_second/dependencies.dv8-dsm-v3.json"
echo ""
echo "Next steps:"
echo "  1. Open any .dv8-dsm-v3.json file in DV8 Explorer for visualization"
echo "  2. See README.md for how to run on your own projects"
echo ""
