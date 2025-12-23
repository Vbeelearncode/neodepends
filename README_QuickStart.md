# NeoDepends (FreeworkEarth fork)

NeoDepends extracts code entities and dependency relationships from a project and exports them in machine-readable formats (SQLite/DSM) for architecture analysis.

This repository is a fork that adds a Python-focused DV8 export pipeline (post-processing + DV8 DSM export helper scripts under `tools/`) while keeping the upstream NeoDepends CLI intact.

## Quick Start: Analyze Fine-Grain Entities

The easiest way to analyze fine-grain code entities (classes, methods, fields, functions) and their dependencies is using the interactive script:

```bash
./run_dependency_analysis.sh
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

**Results:** Open the generated `dependencies.<resolver>.filtered.dv8-dsm-v3.json` file in [DV8 Explorer] to visualize your code dependencies.

---

## The Rest of the original README (may be adjusted accordingly: introduce neodepends, guide on how to directly use neodepends and the python script with the flags, examples)