# Dependency Analyzer

Analyze dependencies between entities (classes, functions, files) in your source code.

## Quick Start

1. **Extract the release archive**:
   ```bash
   tar -xzf neodepends-v*.tar.gz
   cd neodepends-v*
   ```

2. **Run the analyzer**:
   ```bash
   ./dependency-analyzer
   ```

   **macOS only**: If blocked, run `xattr -d com.apple.quarantine ./dependency-analyzer` first

3. **Follow the prompts**:
   - Enter path to your code repository
   - Enter output directory name
   - Choose language: `python` or `java`

4. **View results**:
   - Open `analysis-result.json` in the output directory to view the dependency dsm
   - The `data/` folder contains database files and raw data

## macOS Users

If macOS blocks the executable, run:
```bash
xattr -d com.apple.quarantine ./dependency-analyzer
```

The tool will automatically handle the rest.

## Requirements

- Python 3.7+ (must be in PATH)
- Java 11+ (only needed for analyzing Java projects)

## Supported Languages

- Python
- Java

## Output

The tool generates dependency graphs in DV8-compatible JSON format, showing relationships between:
- Files
- Classes
- Methods/Functions
- Fields/Variables
