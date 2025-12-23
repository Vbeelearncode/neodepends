# `--filter-architecture`

This document describes the “architecture DSM” filtering mode used by `tools/neodepends_python_export.py`.

## What this mode is for

Python is highly dynamic, so a “complete runtime dependency graph” is not generally computable from source alone.

`--filter-architecture` is a *semantic choice* that produces:
- a low-noise DSM suitable for DV8 visualization
- stable, comparable outputs across resolvers (Depends vs StackGraphs)
- a DSM that matches the *handcount ground truth rules* used in the TrainTicket toy projects in this workspace

This is closer to “architecture coupling” than “everything that might happen at runtime”.

## What the mode does

### 1) Canonical DV8 node naming

Nodes are emitted in a path-like hierarchy so DV8 shows structure without requiring clustering JSON:

- `<file>/module (Module)`
- `<file>/FUNCTIONS/<fn> (Function)`
- `<file>/CLASSES/<C> (Class)`
- `<file>/CLASSES/<C>/CONSTRUCTORS/__init__ (Constructor)`
- `<file>/CLASSES/<C>/METHODS/<m> (Method)`
- `<file>/CLASSES/<C>/FIELDS/<f> (Field)`

### 2) Keep only the “core 5” dependency kinds

- `Import`
- `Extend`
- `Create`
- `Call`
- `Use`

### 3) Enforce strict shapes (source-kind -> target-kind)

- `Import`: File -> File
- `Extend`: Class -> Class
- `Create`: Method/Constructor -> Class
- `Call`: Method/Constructor -> Method
- `Use`: Method/Constructor -> Field, *only for fields owned by the same class* (the “self.field” signal)

This means “type-use” (`Method -> Class Use`) is treated as noise by default.

### 4) Internal-only, unique-edge DSM

- Drops external targets (internal-only DSM)
- Deduplicates to unique `(src, tgt, kind)` edges (no call-site counting)
- Applies deterministic ordering of nodes to make visual comparisons easier

## Why this is not a perfect “runtime ground truth” for arbitrary Python

This mode intentionally ignores or can’t reliably model patterns where the runtime target is not obvious from syntax:

- Dynamic dispatch: `obj.do()` can call different implementations depending on runtime type.
- Monkeypatching: `module.func = other_func` (or tests patching methods) changes the target without changing call syntax.
- `getattr`: `getattr(obj, name)()` picks the method name at runtime.
- Decorators: `@decorator` can replace a function with a wrapper; the call “looks” like `f()` but runs different code.
- Properties/descriptors: `obj.x` can execute code (`@property`), so a “field read” can hide calls.
- Metaclasses/class decorators: can inject members after parsing, so entities may not exist directly in source text.
- `__getattr__` / `__getattribute__`: any attribute access can be intercepted and redirected.
- Dynamic imports: `importlib.import_module("pkg." + name)` makes imports depend on runtime strings.
- `eval` / `exec`: code is created at runtime.
- Plugin registries / DI containers: `registry[name]()` hides which function/class is actually called.

## Suggested “quasi-static” next step (research idea)

If you want to approximate dynamic behavior while keeping a stable DSM, combine signals:

1) Start with the filtered static DSM (this mode).
2) Add runtime edges from tracing (optional, as weights/probabilities, not absolute truth):
   - run representative workloads / tests
   - capture observed call edges (e.g. via `sys.setprofile`, decorators, or import hooks)
3) Merge and cluster:
   - keep static edges as baseline
   - add dynamic edges as “confidence-weighted” additions
   - evaluate how clustering stability changes as weights change

This yields an “architecture DSM + observed dynamics” view (quasi-static), without claiming full soundness.