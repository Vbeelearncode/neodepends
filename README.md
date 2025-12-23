# NeoDepends - PyFork (FreeworkEarth fork)

NeoDepends extracts code entities and dependency relationships from a project and exports them in machine-readable formats (SQLite/DSM) for architecture analysis.

This repository is a fork that adds a Python-focused DV8 export pipeline (post-processing + DV8 DSM export helper scripts under `tools/`) while keeping the upstream NeoDepends CLI intact.

## Install

### Release bundle (recommended)

Download a release artifact from this fork and unzip it. The bundle includes:

- `neodepends` executable
- `depends.jar` (required for `--depends`)
- `languages/` (Tree-sitter tagging + StackGraphs definitions)
- `tools/` (Python DV8 export helpers)
- `examples/` (toy projects)

## QuickStart: Automated Examples

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
bash QuickStart_dependency_analysis_examples.sh
```

This will analyze 4 example TrainTicketSystem projects (2 Python, 2 Java) and save DV8 DSM files to `RESULTS_QuickStart_Examples/`.

**Output files (open in DV8 Explorer):**

- `RESULTS_QuickStart_Examples/python_toy_first/dependencies.stackgraphs_ast.filtered.dv8-dsm-v3.json`
- `RESULTS_QuickStart_Examples/python_toy_second/dependencies.stackgraphs_ast.filtered.dv8-dsm-v3.json`
- `RESULTS_QuickStart_Examples/java_toy_first/dependencies.dv8-dsm-v3.json`
- `RESULTS_QuickStart_Examples/java_toy_second/dependencies.dv8-dsm-v3.json`

## Run on Your Own Projects
 
### Python Projects

**Best results:** Use `--resolver stackgraphs` with `--stackgraphs-python-mode ast` for accurate Python dependency extraction.

**macOS / Linux:**

```bash
cd /path/to/neodepends
python3 tools/neodepends_python_export.py \
  --neodepends-bin ./neodepends \
  --input /path/to/your/python/package \
  --output-dir /path/to/output \
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
  --neodepends-bin .\neodepends.exe `
  --input C:\path\to\your\python\package `
  --output-dir C:\path\to\output `
  --resolver stackgraphs `
  --stackgraphs-python-mode ast `
  --dv8-hierarchy structured `
  --filter-architecture `
  --filter-stackgraphs-false-positives
```

**Note:** On Windows, you can optionally add the neodepends directory to your PATH environment variable to run commands from anywhere. Otherwise, make sure to `cd` to the neodepends directory first.

### Single Python File Analysis

You can analyze individual Python files instead of entire directories. This is useful for large single-file scripts or when you want to focus on a specific module.

**macOS / Linux:**

```bash
cd /path/to/neodepends
python3 tools/neodepends_python_export.py \
  --neodepends-bin ./neodepends \
  --input /path/to/your/file.py \
  --output-dir /path/to/output \
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
  --neodepends-bin .\neodepends.exe `
  --input C:\path\to\your\file.py `
  --output-dir C:\path\to\output `
  --resolver stackgraphs `
  --stackgraphs-python-mode ast `
  --dv8-hierarchy structured `
  --filter-architecture `
  --filter-stackgraphs-false-positives
```

**Note:** Single-file analysis will only show dependencies visible within that file. For complete dependency graphs including imports from other modules in your project, analyze the entire package directory instead.

### Java Projects

**Best results:** Use `--depends` with `--depends-jar ./artifacts/depends.jar` for accurate Java dependency extraction.

**macOS / Linux:**

```bash
cd /path/to/neodepends

# Step 1: Extract dependencies
./neodepends \
  --input /path/to/java/src \
  --output /path/to/output/dependencies.db \
  --format sqlite \
  --resources entities,deps,contents \
  --langs java \
  --depends \
  --depends-jar ./artifacts/depends.jar \
  --force

# Step 2: Export to DV8 format
python3 tools/export_dv8_from_neodepends_db.py \
  --db /path/to/output/dependencies.db \
  --out /path/to/output/dependencies.dv8-dsm-v3.json \
  --name "My Java Project"
```

**Windows:**

```powershell
cd C:\path\to\neodepends

# Step 1: Extract dependencies
.\neodepends.exe `
  --input C:\path\to\java\src `
  --output C:\path\to\output\dependencies.db `
  --format sqlite `
  --resources entities,deps,contents `
  --langs java `
  --depends `
  --depends-jar .\artifacts\depends.jar `
  --force

# Step 2: Export to DV8 format
py -3 tools\export_dv8_from_neodepends_db.py `
  --db C:\path\to\output\dependencies.db `
  --out C:\path\to\output\dependencies.dv8-dsm-v3.json `
  --name "My Java Project"
```

**Note:** Java analysis requires a Java runtime to be installed on your system.


## QuickStart: Interactive Script

For a guided analysis experience, use the interactive script that prompts you for all required settings:

**macOS / Linux:**

```bash
cd /path/to/neodepends
chmod +x run_dependency_analysis.sh
./run_dependency_analysis.sh
```

**Windows:**

```powershell
cd C:\path\to\neodepends
bash run_dependency_analysis.sh
```

The script will prompt you for:

- **NeoDepends binary path** - Press Enter to use `./neodepends` (default) or provide a custom path
- **Input repository path** - Path to your project directory (tab completion enabled)
- **Output directory path** - Where to save results (tab completion enabled)
- **Language** - `python` or `java`
- **Model/Resolver** - `d` or `s` (shortcuts for `depends` or `stackgraphs`)

The script automatically applies recommended settings:

- Structured DV8 hierarchy for easy navigation
- Architecture filtering for cleaner results
- For Python with StackGraphs: AST-based classification and false positive filtering

Example session:

```bash
$ ./run_dependency_analysis.sh
Enter neodepends binary path [default: ./neodepends]:
Enter input repository path: examples/TrainTicketSystem_TOY_PYTHON_FIRST/tts
Enter output directory path: ./test-package
Enter language (python or java): python
Enter model (d/D/depends or s/S/stackgraphs): s
...
```

**Results:** Open the generated `dependencies.<resolver>.filtered.dv8-dsm-v3.json` file in DV8 Explorer to visualize your code dependencies.

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
