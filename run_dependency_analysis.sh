#!/bin/bash

# NeoDepends Dependency Analysis Runner
# This script prompts for inputs and runs the dependency analysis pipeline

set -e  # Exit on error

# Get script directory to find tools and neodepends binary
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Prompt for neodepends binary path (with default)
DEFAULT_NEODEPENDS="./neodepends"
read -e -p "Enter neodepends binary path [default: $DEFAULT_NEODEPENDS]: " NEODEPENDS_BIN
NEODEPENDS_BIN=$(echo "$NEODEPENDS_BIN" | xargs)  # Trim whitespace
if [ -z "$NEODEPENDS_BIN" ]; then
    NEODEPENDS_BIN="$DEFAULT_NEODEPENDS"
fi

# Validate neodepends binary exists
if [ ! -f "$NEODEPENDS_BIN" ]; then
    echo "Error: NeoDepends binary not found at: $NEODEPENDS_BIN"
    exit 1
fi

# Make neodepends executable and remove Mac quarantine attribute
chmod +x "$NEODEPENDS_BIN"
# Remove Mac quarantine attribute if on macOS
if [[ "$OSTYPE" == "darwin"* ]]; then
    xattr -dr com.apple.quarantine "$NEODEPENDS_BIN" 2>/dev/null || true
fi

# Prompt for input repository (tab completion enabled)
read -e -p "Enter input repository path: " INPUT_REPO
if [ ! -d "$INPUT_REPO" ]; then
    echo "Error: Input repository path does not exist: $INPUT_REPO"
    exit 1
fi

# Prompt for output location (tab completion enabled)
read -e -p "Enter output directory path: " OUTPUT_DIR
OUTPUT_DIR=$(echo "$OUTPUT_DIR" | xargs)  # Trim whitespace
if [ -z "$OUTPUT_DIR" ]; then
    echo "Error: Output directory cannot be empty"
    exit 1
fi

# Create output directory if it doesn't exist
mkdir -p "$OUTPUT_DIR"

# Prompt for language
read -p "Enter language (python, java, or typescript): " LANGUAGE
LANGUAGE=$(echo "$LANGUAGE" | tr '[:upper:]' '[:lower:]')  # Convert to lowercase
if [ "$LANGUAGE" != "python" ] && [ "$LANGUAGE" != "java" ] && [ "$LANGUAGE" != "typescript" ]; then
    echo "Error: Language must be 'python', 'java', or 'typescript'"
    exit 1
fi

# Prompt for model/resolver (accept shortcuts: d/D/depends or s/S/stackgraphs)
# COMMENTED OUT: Manual resolver selection (kept for future testing)
# read -p "Enter model (d/D/depends or s/S/stackgraphs): " MODEL
# MODEL=$(echo "$MODEL" | tr '[:upper:]' '[:lower:]')  # Convert to lowercase
# # Normalize shortcuts to full names
# case "$MODEL" in
#     d|depends)
#         MODEL="depends"
#         ;;
#     s|stackgraphs)
#         MODEL="stackgraphs"
#         ;;
#     *)
#         echo "Error: Model must be 'd', 'depends', 's', or 'stackgraphs'"
#         exit 1
#         ;;
# esac

# Auto-select resolver based on language
LANGS_ARG="$LANGUAGE"
if [ "$LANGUAGE" == "python" ]; then
    MODEL="stackgraphs"
    echo "Auto-selected resolver: stackgraphs (for Python)"
elif [ "$LANGUAGE" == "java" ]; then
    MODEL="depends"
    echo "Auto-selected resolver: depends (for Java)"
elif [ "$LANGUAGE" == "typescript" ]; then
    MODEL="stackgraphs"
    LANGS_ARG="typescript,tsx"
    echo "Auto-selected resolver: stackgraphs (for TypeScript/TSX)"
else
    echo "Error: Unsupported language: $LANGUAGE"
    exit 1
fi

# Build the command
CMD="python3 tools/neodepends_python_export.py"
CMD="$CMD --neodepends-bin \"$NEODEPENDS_BIN\""
CMD="$CMD --input \"$INPUT_REPO\""
CMD="$CMD --output-dir \"$OUTPUT_DIR\""
CMD="$CMD --langs $LANGS_ARG"
CMD="$CMD --resolver $MODEL"

# Add default flags
CMD="$CMD --dv8-hierarchy structured"
CMD="$CMD --filter-architecture"

# For Python with stackgraphs, add stackgraphs-specific flags
if [ "$LANGUAGE" == "python" ] && [ "$MODEL" == "stackgraphs" ]; then
    CMD="$CMD --stackgraphs-python-mode ast"
    CMD="$CMD --filter-stackgraphs-false-positives"
fi

if [ "$LANGUAGE" == "typescript" ] && [ "$MODEL" == "stackgraphs" ]; then
    CMD="$CMD --stackgraphs-typescript-mode ast"
fi

echo ""
echo "Running dependency analysis with the following settings:"
echo "  NeoDepends binary: $NEODEPENDS_BIN"
echo "  Input repository: $INPUT_REPO"
echo "  Output directory: $OUTPUT_DIR"
echo "  Language: $LANGUAGE"
echo "  Model/Resolver: $MODEL"
echo ""
echo "Command: $CMD"
echo ""

# Execute the command
eval $CMD

# Determine the output filename based on resolver
if [ "$MODEL" == "depends" ]; then
    RESOLVER_NAME="depends"
elif [ "$MODEL" == "stackgraphs" ] && [ "$LANGUAGE" == "python" ]; then
    RESOLVER_NAME="stackgraphs_ast"
elif [ "$MODEL" == "stackgraphs" ] && [ "$LANGUAGE" == "typescript" ]; then
    RESOLVER_NAME="stackgraphs_ast"
else
    RESOLVER_NAME="stackgraphs"
fi
OUTPUT_FILE="$OUTPUT_DIR/dependencies.$RESOLVER_NAME.filtered.dv8-dsm-v3.json"

echo ""
echo "================================================================================"
echo "Dependency analysis complete!"
echo ""
echo "Results saved to: $OUTPUT_DIR"
echo ""
echo "To visualize results in DV8 Explorer, open:"
echo "  $OUTPUT_FILE"
echo "================================================================================"

