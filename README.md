# Neodepends

Quickly extract entities, dependencies, changes, and file contents from a software project.

Neodepends can scan a working directory or one or more git commits and export the results as JSONL, SQLite, CSV tables, or a design structure matrix (DSM).

## Build

```bash
cargo build --release
```

## Usage

```bash
cargo run -- --help
```

Or download a recent [release](https://github.com/jlefever/neodepends/releases) and run:

```bash
neodepends --help
```

## What Neodepends extracts

Neodepends can export four resource types:

- `entities`: source code entities such as files, classes, methods, constructors, and fields
- `deps`: syntactic dependencies between entities such as calls, imports, inheritance, and use relationships
- `changes`: records of which entities changed in which commits
- `contents`: raw file contents

Structural resources are `entities`, `deps`, and `contents`. Historical resources are `changes`.

## Quick start

Scan the current working directory and write a DSM:

```bash
neodepends --output matrix.json WORKDIR
```

Scan a repository at `HEAD`:

```bash
neodepends --output matrix.json HEAD
```

Scan a project from disk without relying on git:

```bash
neodepends --input /path/to/project --output matrix.json WORKDIR
```

## Example

Here is an example of how to generate a design structure matrix from a software repository:

```bash
git clone https://github.com/apache/deltaspike
cd deltaspike
neodepends --output matrix.json HEAD
```

`HEAD` can be replaced with any commit reference such as a branch, tag, short hash, or full hash. If you want to scan directly from disk instead of from git history, use `WORKDIR`.

Neodepends still works if the project is not a git repository.

## Historical analysis and co-change

To calculate co-change, pass multiple commits. Neodepends works well with [`git rev-list`](https://git-scm.com/docs/git-rev-list):

```bash
neodepends --output matrix.json $(git rev-list deltaspike-1.9.6 -n 300)
```

This extracts structural data from the first commit in the list and uses the rest to compute `changes` and DSM co-change cells.

If you also want structural data from additional commits, add `--structure`:

```bash
neodepends --output out.jsonl --format jsonl $(git rev-list HEAD -n 100) --structure HEAD~10 --structure HEAD~20
```

If your shell makes command substitution awkward, you can pass a file of commits instead:

```bash
git rev-list HEAD -n 100 > commits.txt
neodepends --output out.jsonl --format jsonl commits.txt
```

## Output formats

Supported output formats:

- `jsonl`
- `sqlite`
- `csvs`
- `dsm-v1`
- `dsm-v2`

If `--format` is omitted, Neodepends infers it from the output path when possible. In particular:

- `.json` => `dsm-v2`
- `.jsonl` => `jsonl`
- `.db` => `sqlite`

### DSM formats

`dsm-v1` and `dsm-v2` both emit:

- `variables`: the entities represented in the matrix
- `cells`: relationships between pairs of variables

Use `dsm-v1` when you want a file-level DSM. It implies `--file-level`.

Use `dsm-v2` when you want entity-level extraction in the DSM. This is the format to use when you want classes, methods, constructors, fields, and other non-file entities to appear directly in `variables`.

If you do not specify a dependency resolver, Neodepends will choose the appropriate resolver for each supported language. When you want to force one explicitly, prefer `--stackgraphs`.

Examples:

```bash
# File-level DSM
neodepends --output matrix-v1.json --format dsm-v1 HEAD

# Entity-level DSM (dsm-v2 is inferred from .json)
neodepends --output matrix-v2.json HEAD

# Force file-level output even with DSM v2
neodepends --output matrix-files.json --format dsm-v2 --file-level HEAD
```

If you want the raw entity table instead of a DSM, export `entities` directly:

```bash
neodepends --output entities.jsonl --format jsonl --resources entities WORKDIR
```

Or write entities and deps into SQLite:

```bash
neodepends --output out.db --format sqlite --resources entities --resources deps WORKDIR
```

## Dependency resolution

Neodepends supports two dependency resolvers:

- `--stackgraphs`
- `--depends`

If neither is specified, Neodepends will choose the resolver for each language.

### Enhancement and heuristics

By default, Neodepends applies query-driven dependency enhancement after raw resolution.

- `--no-enhance`: skip all enhancement
- `--heuristics`: enable additional language-specific heuristics

Examples of heuristic recovery include:

- Java constructor field assignments
- Java `super()` / `this()` delegation
- Java `@Override`
- Python dataclass field type dependencies

## Languages

Supported languages:

- `c`
- `cpp`
- `go`
- `java`
- `javascript`
- `kotlin`
- `python`
- `ruby`
- `typescript`

Restrict scanning to a subset with `--langs`:

```bash
neodepends --output out.jsonl --format jsonl --langs python --langs java WORKDIR
```

## Useful commands

```bash
# Build release binary
cargo build --release

# Build debug binary
cargo build

# Run tests
./tests/run_all_final_tests.sh

# Check lint warnings
cargo clippy

# Format code
cargo fmt
```

## Help

```text
Scan a project and extract structural and historical information.

Usage: neodepends [OPTIONS] --output <OUTPUT> [COMMIT]... [-- <PATH>...]

Arguments:
  [COMMIT]...
          Commits to be scanned for resources.
          
          Defaults to WORKDIR if not specified. If input is a bare repository,
          then it will default to HEAD. Entities, deps, and contents will only
          be extracted from the first commit.

  [PATH]...
          Patterns that each path must match to be scanned

Options:
  -o, --output <OUTPUT>
          The path of the output file or directory

  -f, --force
          Overwrite the output file or directory if it already exists

  -i, --input <INPUT>
          The root of the project/repository to scan

      --format <FORMAT>
          Output format: csvs, jsonl, sqlite, dsm-v1, dsm-v2

  -r, --resources <RESOURCES>
          Resources to export: entities, deps, changes, contents

      --all-entities
          Extract entities from historical commits in addition to structural

      --file-level
          Always report at the file-level

      --structure <COMMIT>
          Scan these commits for structural data

  -l, --langs <LANGS>
          Only scan the provided languages

Dependency options:
  -S, --stackgraphs
          Enable Stack Graphs dependency resolution

  -D, --depends
          Enable Depends dependency resolution

      --stackgraphs-python-mode <STACKGRAPHS_PYTHON_MODE>
          Python Stack Graphs mode: use-only, ast

      --stackgraphs-ref-timeout-secs <STACKGRAPHS_REF_TIMEOUT_SECS>
          Per-reference Stack Graphs stitching timeout

      --no-enhance
          Skip dependency enhancement

      --heuristics
          Enable heuristic dependency enhancement
```

For the full current CLI reference, run:

```bash
neodepends --help
```
