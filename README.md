# NeoDepends - PyFork (FreeworkEarth fork)

NeoDepends extracts code entities and dependency relationships from a project and exports them in machine-readable formats (SQLite/DSM) for architecture analysis.

This repository is a fork that adds a Python-focused DV8 export pipeline (post-processing + DV8 DSM export helper scripts under `tools/`) while keeping the upstream NeoDepends CLI intact.

## Install

### Release bundle (recommended)

Download the latest release artifact from this fork and unzip it. The bundle includes:

- `neodepends` executable
- `depends.jar` (required for `--depends`)
- `languages/` (Tree-sitter tagging + StackGraphs definitions)
- `tools/` (Python DV8 export helpers)
- `examples/` (toy projects)


## QuickStart Release Bundle: One-Command Setup & Analysis

### 1. First-Time Setup (One-Time Only)

Run the automated setup script to check dependencies:

**macOS / Linux:**

```bash
python3 setup.py
```

**Windows:**

```powershell
py -3 setup.py
```

This script will:

- Verify Python version (3.7+ required)
- Check for NeoDepends binary
- Check for Java (optional, needed for Java analysis)
- Install any required Python packages
- Provide clear next steps

### 2. Run Dependency Analysis (Cross-Platform)

**Recommended for all platforms** - Pure Python script that works everywhere:

```bash
python3 run_dependency_analysis.py
```

**Alternative platform-specific scripts:**

**macOS / Linux (Bash):**

```bash
chmod +x run_dependency_analysis.sh
./run_dependency_analysis.sh
```

**Windows (PowerShell):**

```powershell
.\run_dependency_analysis.ps1
```

**Windows (Git Bash)** - Requires [Git Bash](https://git-scm.com/download/win):

```bash
bash run_dependency_analysis.sh
```

All scripts provide identical functionality. The Python version is recommended for maximum compatibility.

The script will prompt you for:

- **NeoDepends binary path** - Press Enter to use `./neodepends` (default) or provide a custom path
- **Input repository path** - Path to your project directory (tab completion enabled)
- **Output directory path** - Where to save results (tab completion enabled)
- **Language** - `python`, `java`, or `typescript` (resolver auto-selected based on language). The `typescript` choice scans both `.ts` and `.tsx` files.

The script automatically applies recommended settings:

- Structured DV8 hierarchy for easy navigation
- Architecture filtering for cleaner results
- For Python with StackGraphs: AST-based classification and false positive filtering
- For TypeScript/TSX: Rust-side AST classification (Import/Annotation/Extend/Implement/Create/Call) and false-positive filtering — all performed by the core binary with no Python post-processing

Example session:

```bash
$ ./run_dependency_analysis.sh
Enter neodepends binary path [default: ./neodepends]:
Enter input repository path: examples/TrainTicketSystem_TOY_PYTHON_FIRST/tts
Enter output directory path: ./test-package
Enter language (python, java, or typescript): python
Auto-selected resolver: stackgraphs (for Python)
...
```

**Results:** Open the generated `dependencies.<resolver>.filtered.dv8-dsm-v3.json` file in DV8 Explorer to visualize your code dependencies.

## QuickStart: Automated Analysis on Examples

The easiest way to get started is to run NeoDepends on the included example projects:

**macOS / Linux:**

```bash
cd /path/to/neodepends
chmod +x QuickStart_dependency_analysis_examples.sh
./QuickStart_dependency_analysis_examples.sh
```

**Windows:**

```powershell
cd C:\path\to\neodepends
.\QuickStart_dependency_analysis_examples.ps1
```

This will analyze 4 example TrainTicketSystem projects (2 Python, 2 Java) and save DV8 DSM files to `RESULTS_QuickStart_Examples/`.

**Output files (open in DV8 Explorer):**

- `RESULTS_QuickStart_Examples/python_toy_first/dependencies.stackgraphs_ast.filtered.dv8-dsm-v3.json`
- `RESULTS_QuickStart_Examples/python_toy_second/dependencies.stackgraphs_ast.filtered.dv8-dsm-v3.json`
- `RESULTS_QuickStart_Examples/java_toy_first/dependencies.dv8-dsm-v3.json`
- `RESULTS_QuickStart_Examples/java_toy_second/dependencies.dv8-dsm-v3.json`

## Run on Your Own Projects

### Recommended: Use Config Presets (Simple)

The easiest way to run NeoDepends is using `--langs` flag with `--config default`:

**Python Projects:**

```bash
cd /path/to/neodepends
python3 tools/neodepends_python_export.py \
  --input /path/to/your/python/package \
  --output-dir /path/to/output \
  --langs python \
  --config default
```

**Single Python File:**

```bash
python3 tools/neodepends_python_export.py \
  --input /path/to/your/file.py \
  --output-dir /path/to/output \
  --langs python \
  --config default
```

**Java Projects:**

```bash
python3 tools/neodepends_python_export.py \
  --input /path/to/java/src \
  --output-dir /path/to/output \
  --langs java \
  --config default
```

**What `--langs <language> --config default` does:**

- Uses the `--langs` flag to determine which language preset to apply
- Applies best-practice resolver (stackgraphs for Python, depends for Java)
- Enables structured DV8 hierarchy for easy navigation
- Enables architecture filtering for cleaner results
- Enables false positive filtering (Python only)

**Automatic language detection:**

If you want automatic language detection from file extensions, use `--config automatic` (no `--langs` needed):

```bash
python3 tools/neodepends_python_export.py \
  --input /path/to/your/code \
  --output-dir /path/to/output \
  --config automatic
```

**Windows Users:** Replace `python3` with `py -3` and use backslashes (`\`) in paths.

**All available presets:**

- `--langs <language> --config default` - Use language flag to apply best practices (recommended)
- `--config automatic` - Auto-detect language from file extensions
- `--langs python --config python` - Explicitly use Python best practices
- `--langs java --config java` - Explicitly use Java best practices
- `--config manual` - Specify all options yourself (advanced, default)


---

## CLI Help

View all available options:

**macOS / Linux:**

```bash
cd /path/to/neodepends
./neodepends --help
python3 tools/neodepends_python_export.py --help
```

**Windows:**

```powershell
cd C:\path\to\neodepends
.\neodepends.exe --help
py -3 tools\neodepends_python_export.py --help
```

## Important Notes

- **Release bundles:** If you downloaded the release bundle, do not run `cargo build` inside it (there is no `Cargo.toml`).
- **Release tags:** This fork uses a `-pyfork` suffix to avoid confusion with upstream (example: `v0.0.11-pyfork`).
- **macOS Gatekeeper:** If macOS blocks the executable after download/unzip, use System Settings → Privacy & Security to allow it, or run:

```bash
cd /path/to/neodepends
chmod +x ./neodepends
xattr -dr com.apple.quarantine ./neodepends
```

## Build from Source

Requires Rust and Cargo.

```bash
git clone <repository-url>
cd neodepends
cargo build --release
./target/release/neodepends --help
```

If you built from source and want to use `--depends`, either place `depends.jar` next to the executable or pass it explicitly:

```bash
./target/release/neodepends --depends --depends-jar ./artifacts/depends.jar --help
```

---

## Advanced Usage

### Direct CLI Usage (Core NeoDepends)

For advanced users who want to use NeoDepends directly without the Python export helpers:

**macOS / Linux:**

```bash
cd /path/to/neodepends
./neodepends \
  --input /path/to/project \
  --output out.sqlite \
  --format sqlite \
  --resources entities,deps,contents \
  --langs java \
  --depends \
  --depends-jar ./artifacts/depends.jar
```

**Windows:**

```powershell
cd C:\path\to\neodepends
.\neodepends.exe `
  --input C:\path\to\project `
  --output out.sqlite `
  --format sqlite `
  --resources entities,deps,contents `
  --langs java `
  --depends `
  --depends-jar .\artifacts\depends.jar
```

**Notes:**

- Depends (`--depends`) requires a Java runtime (and ships a `depends.jar` in this fork's bundle).
- StackGraphs (`--stackgraphs`) is a name-binding resolver; it is available without Java.

### Git Mode (Optional)

You can also scan commits from a git repository instead of scanning directly from disk:

```bash
cd /path/to/neodepends
./neodepends --output out.dsm-v2.json --format dsm-v2 --depends HEAD
```

`HEAD` can be replaced with any commit-ish, or with `WORKDIR` to force disk mode.

### DV8 Export Helper (Python)

For DV8 Explorer workflows, use `tools/neodepends_python_export.py`. It runs NeoDepends, applies optional Python post-processing, and exports DV8-openable DSM JSONs (full-project + file-level + per-file).

**macOS / Linux:**

```bash
cd /path/to/neodepends
python3 tools/neodepends_python_export.py \
  --input examples/TrainTicketSystem_TOY_PYTHON_FIRST/tts \
  --output-dir /tmp/neodepends_tts_toy1 \
  --resolver stackgraphs \
  --stackgraphs-python-mode ast \
  --dv8-hierarchy structured \
  --filter-architecture \
  --filter-stackgraphs-false-positives
```

**Windows:**

```powershell
cd C:\path\to\neodepends
py -3 tools\neodepends_python_export.py `
  --input examples\TrainTicketSystem_TOY_PYTHON_FIRST\tts `
  --output-dir C:\tmp\neodepends_tts_toy1 `
  --resolver stackgraphs `
  --stackgraphs-python-mode ast `
  --dv8-hierarchy structured `
  --filter-architecture `
  --filter-stackgraphs-false-positives
```

Key outputs (naming scheme “DV8 DSM v3”):

- `dependencies.<resolver>.filtered.db`
- `dependencies.<resolver>.filtered.dv8-dsm-v3.json` (full-project DSM)
- `dependencies.<resolver>.filtered.file.dv8-dsm-v3.json` (file-level DSM)
- `dv8_deps/*.dv8-dependency.json` (per-file DSMs)

Where `<resolver>` is one of:

- `depends`
- `stackgraphs_ast`
- `stackgraphs_use_only`

**Which file to open in DV8:**

- Open `dependencies.<resolver>.filtered.dv8-dsm-v3.json` for the full, drill-down DSM (recommended for most analysis).
- Open `dependencies.<resolver>.filtered.file.dv8-dsm-v3.json` for a file-level DSM.
- Per-file DSMs are in the `dv8_deps/` subdirectory.

**Note on binary location:**

- In the release bundle: the `neodepends` executable is in the root directory of the extracted archive.
- When building from source: the executable is at `target/release/neodepends`.

## Examples

See `examples/README.md` for bundled toy projects and runnable commands.

---

## Contributors

This fork enabling dependency extraction with python was developed and maintained by:

- **Christoph Haring** (<charing@hawaii.edu>) - Python export pipeline, DV8 integration, single-file analysis
- **Bao Vuong** (<baovg.a1.k2023@gmail.com>) - Python tooling and testing

Original NeoDepends core by **Jason Lefever** (<jason.titus.lefever@gmail.com>)

### Citation

If you use this fork in research, please cite both the original NeoDepends project and this Python fork extension.
