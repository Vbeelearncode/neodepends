#!/usr/bin/env python3
"""
NeoDepends Python pipeline:
1) Run neodepends -> SQLite DB (entities/deps/contents)
2) Run Python enhancement -> normalize Field parent_id + Method->Field Use deps
3) Export DV8 dependency JSON per source file

Architecture DSM filtering
--------------------------

This script supports an optional “architecture DSM” filtering mode for DV8 exports so results are:
- comparable across runs and across resolvers (Depends vs StackGraphs)
- lower-noise for architecture visualization and clustering

CLI flag:
  - `--filter-architecture` (alias: `--align-handcount` for backward compatibility)

What this filter does (high level):
  - Canonicalizes entity names to a stable DV8 hierarchy:
      <file>/module (Module)
      <file>/FUNCTIONS/<fn> (Function)
      <file>/CLASSES/<C> (Class)
      <file>/CLASSES/<C>/CONSTRUCTORS/__init__ (Constructor)
      <file>/CLASSES/<C>/METHODS/<m> (Method)
      <file>/CLASSES/<C>/FIELDS/<f> (Field)
  - Keeps only the “core 5” dependency kinds used in this project:
      Import, Extend, Create, Call, Use
  - Enforces strict shapes (source-kind -> target-kind):
      Import: File -> File
      Extend: Class -> Class
      Create: Method/Constructor -> Class
      Call:   Method/Constructor -> Method
      Use:    Method/Constructor -> Field, and only for fields owned by the same class
              (the “self.field” signal; cross-object attribute reads like `x.y` are excluded)
  - Drops external targets (internal-only DSM), deduplicates to unique edges, and applies
    deterministic ordering of nodes for easy DV8 visual comparison.

Important limitation:
  This filter is a semantic choice. It will not perfectly represent “all runtime dependencies”
  for arbitrary Python projects (Python is highly dynamic). It is designed for architecture-level
  DSMs, not full program analysis.

Example:
  python3 tools/neodepends_python_export.py \\
    --neodepends-bin ./target/release/neodepends \\
    --input /path/to/tts \\
    --output-dir /path/to/results/NEODEP_V008 \\
    --resolver depends

Note: If you pass a Python package directory (e.g. `.../tts`) but your code imports
`from tts.x import Y`, StackGraphs resolves imports best when NeoDepends runs from the
parent directory so files are named `tts/...` internally. This script can auto-detect
that and still export DV8/DBs only for the requested focus dir.
"""

from __future__ import annotations

import argparse
import datetime as _dt
import json
import sqlite3
import subprocess
import sys
import shutil
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable, Dict, Iterable, List, Optional, Sequence, Set, Tuple


def _get_python_executable() -> str:
    """
    Get the correct Python executable to use for subprocess calls.

    Inside PyInstaller bundles, sys.executable points to the bundled executable,
    not Python. We need to find the actual Python interpreter.

    Raises:
        RuntimeError: If Python interpreter cannot be found in PATH (PyInstaller only)
    """
    # Check if running in PyInstaller bundle
    if getattr(sys, 'frozen', False):
        # Running in PyInstaller bundle - need to find Python
        # Try common Python executable names
        for name in ['python3', 'python']:
            python_path = shutil.which(name)
            if python_path:
                return python_path

        # No Python found - this is a critical error
        raise RuntimeError(
            "Python interpreter not found in PATH. "
            "The dependency-analyzer requires Python 3.7+ to be installed and available in PATH. "
            "Please ensure Python is installed and added to your system PATH, then try again."
        )
    else:
        # Normal Python execution - use sys.executable
        return sys.executable


@dataclass(frozen=True)
class DbEntity:
    id: bytes
    parent_id: Optional[bytes]
    kind: str
    name: str
    content_id: bytes


class _Logger:
    def __init__(self, log_path: Path) -> None:
        self.log_path = log_path
        self._fp = log_path.open("w", encoding="utf-8")

    def close(self) -> None:
        self._fp.close()

    def line(self, msg: str = "") -> None:
        sys.stdout.write(msg + "\n")
        sys.stdout.flush()
        self._fp.write(msg + "\n")
        self._fp.flush()

class _StdoutLogger:
    def line(self, msg: str = "") -> None:
        sys.stdout.write(msg + "\n")
        sys.stdout.flush()

    def close(self) -> None:
        return


def _find_dir_named(start: Path, name: str) -> Optional[Path]:
    for candidate in [start, *start.parents]:
        if candidate.name == name:
            return candidate
    return None


def _resolve_path_arg(path: Path, *, prefer_agent_root: bool, must_exist: bool, kind: str) -> Path:
    """
    Resolve a CLI path argument to an absolute path.

    Why: users often run this script from the `neodepends/` repo directory but pass
    workspace-relative paths like `TEST_AUTO/...`. If we interpret those relative
    to the current working directory, NeoDepends scans a non-existent folder and
    produces an empty DB/JSONs.
    """
    if path.is_absolute():
        resolved = path
    else:
        agent_root = _find_dir_named(Path(__file__).resolve(), "AGENT")
        if (
            prefer_agent_root
            and agent_root is not None
            and path.parts
            and path.parts[0] in {"TEST_AUTO", "EXAMPLES_CHRIS"}
        ):
            resolved = agent_root / path
        else:
            resolved = Path.cwd() / path

    resolved = resolved.expanduser().resolve(strict=False)

    if must_exist and not resolved.exists():
        raise FileNotFoundError(
            f"{kind} path does not exist: {resolved}\n"
            f"Tip: pass an absolute path (starts with `/`) or run this script from the `AGENT/` workspace root."
        )

    return resolved


def _connect_ro(db_path: Path) -> sqlite3.Connection:
    return sqlite3.connect(f"file:{db_path.resolve()}?immutable=1", uri=True)

def _connect_rw(db_path: Path) -> sqlite3.Connection:
    # Default sqlite3 connect opens RW and creates the file.
    return sqlite3.connect(str(db_path.resolve()))

def _run_and_tee(cmd: Sequence[str], *, logger: Any) -> float:
    """
    Run a subprocess, streaming stdout+stderr to both terminal and `terminal_output.txt`.

    Returns: elapsed seconds
    """
    start = time.time()
    logger.line(f"[CMD] {' '.join(cmd)}")
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    assert proc.stdout is not None
    for line in proc.stdout:
        logger.line(line.rstrip("\n"))
    rc = proc.wait()
    elapsed = time.time() - start
    if rc != 0:
        raise subprocess.CalledProcessError(rc, list(cmd))
    return elapsed

def _safe_tag(s: str) -> str:
    return s.replace("-", "_").replace(" ", "_")

def _copy_if_exists(src: Path, dest: Path) -> None:
    """
    Best-effort: write convenience copies with resolver/mode in the filename.
    """
    try:
        if not src.exists():
            return
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(src, dest)
    except Exception:
        return


def run_neodepends(
    *,
    neodepends_bin: Path,
    input_dir: Path,
    db_out: Path,
    resolver: str,
    langs: Sequence[str],
    depends_jar: Optional[Path],
    java_bin: Optional[str],
    xmx: Optional[str],
    stackgraphs_python_mode: str,
    logger: Any,
) -> None:
    cmd: List[str] = [
        str(neodepends_bin),
        "--force",
        f"--input={input_dir}",
        f"--output={db_out}",
        "--format=sqlite",
        "--resources=entities,deps,contents",
        f"--langs={','.join(langs)}",
    ]

    if resolver == "depends":
        cmd.append("--depends")
        if depends_jar:
            cmd.append(f"--depends-jar={depends_jar}")
        if java_bin:
            cmd.append(f"--depends-java={java_bin}")
        if xmx:
            cmd.append(f"--depends-xmx={xmx}")
    elif resolver == "stackgraphs":
        cmd.append("--stackgraphs")
        if stackgraphs_python_mode:
            cmd.append(f"--stackgraphs-python-mode={stackgraphs_python_mode}")
    else:
        raise ValueError(f"Unknown resolver: {resolver}")

    _run_and_tee(cmd, logger=logger)


def run_python_enhancement(*, enhance_script: Path, db_path: Path, profile: str, logger: Any) -> None:
    _run_and_tee([_get_python_executable(), str(enhance_script), str(db_path), "--profile", profile], logger=logger)

def run_stackgraphs_false_positive_filter(
    *,
    filter_script: Path,
    input_db: Path,
    output_db: Path,
    logger: Any,
) -> None:
    """
    Filter StackGraphs false positives in a *raw* NeoDepends DB.

    Important: this must run before the Python enhancement step, otherwise the filter might
    delete enhancement-added deps (which intentionally use method_start rows).
    """
    _run_and_tee([_get_python_executable(), str(filter_script), str(input_db), str(output_db)], logger=logger)


def _load_entities(conn: sqlite3.Connection) -> Dict[bytes, DbEntity]:
    cur = conn.cursor()
    rows = cur.execute("SELECT id, parent_id, kind, name, content_id FROM entities").fetchall()
    return {r[0]: DbEntity(id=r[0], parent_id=r[1], kind=r[2], name=r[3], content_id=r[4]) for r in rows}


def _ancestor_class_name(entities: Dict[bytes, DbEntity], entity_id: bytes) -> Optional[str]:
    current = entities.get(entity_id)
    while current and current.parent_id is not None:
        parent = entities.get(current.parent_id)
        if parent and parent.kind == "Class":
            return parent.name
        current = parent
    return None


def _display_name(entities: Dict[bytes, DbEntity], entity_id: bytes) -> str:
    ent = entities[entity_id]
    if ent.kind in {"Method", "Function", "Field"}:
        class_name = _ancestor_class_name(entities, entity_id)
        if class_name:
            return f"{class_name}.{ent.name}"
    return ent.name


def _descendants(conn: sqlite3.Connection, root_id: bytes) -> Set[bytes]:
    cur = conn.cursor()
    rows = cur.execute(
        """
        WITH RECURSIVE tree(id) AS (
          SELECT id FROM entities WHERE id = ?
          UNION ALL
          SELECT e.id FROM entities e JOIN tree t ON e.parent_id = t.id
        )
        SELECT id FROM tree
        """,
        (root_id,),
    ).fetchall()
    return {r[0] for r in rows}

def _ensure_ancestors(entities: Dict[bytes, DbEntity], ids: Set[bytes]) -> Set[bytes]:
    out = set(ids)
    for entity_id in list(ids):
        current = entities.get(entity_id)
        while current and current.parent_id is not None and current.parent_id not in out:
            out.add(current.parent_id)
            current = entities.get(current.parent_id)
    return out

def export_per_file_dbs(
    *,
    db_path: Path,
    out_dir: Path,
    include_incoming_edges: bool,
    only_py: bool,
    focus_prefix: Optional[str],
    include_root_py: bool,
) -> None:
    """
    Create a small, file-scoped SQLite DB for each File entity.

    Goal: make it easy to hand-audit dependencies per file without slicing the big DB manually.
    """
    con = _connect_ro(db_path)
    entities = _load_entities(con)
    cur = con.cursor()

    out = out_dir / "per_file_dbs"
    out.mkdir(parents=True, exist_ok=True)

    file_rows = cur.execute("SELECT id, name FROM entities WHERE kind = 'File' ORDER BY name").fetchall()
    for file_id, file_name in file_rows:
        if only_py and not file_name.endswith(".py"):
            continue
        if focus_prefix is not None:
            if file_name.startswith(focus_prefix):
                pass
            elif include_root_py and "/" not in file_name and file_name.endswith(".py"):
                pass
            else:
                continue

        internal_ids = _descendants(con, file_id)
        if not internal_ids:
            continue

        placeholders = ",".join(["?"] * len(internal_ids))
        if include_incoming_edges:
            dep_rows = cur.execute(
                f"""
                SELECT src, tgt, kind, row, commit_id
                FROM deps
                WHERE src IN ({placeholders}) OR tgt IN ({placeholders})
                """,
                list(internal_ids) + list(internal_ids),
            ).fetchall()
        else:
            dep_rows = cur.execute(
                f"""
                SELECT src, tgt, kind, row, commit_id
                FROM deps
                WHERE src IN ({placeholders})
                """,
                list(internal_ids),
            ).fetchall()

        keep_entity_ids: Set[bytes] = set(internal_ids)
        for src, tgt, _k, _row, _cid in dep_rows:
            keep_entity_ids.add(src)
            keep_entity_ids.add(tgt)

        keep_entity_ids = _ensure_ancestors(entities, keep_entity_ids)

        keep_content_ids: Set[bytes] = set()
        for entity_id in keep_entity_ids:
            ent = entities.get(entity_id)
            if ent:
                keep_content_ids.add(ent.content_id)

        per_db = out / f"{Path(file_name).stem}.neodepends.db"
        if per_db.exists():
            per_db.unlink()

        dst = _connect_rw(per_db)
        dst.executescript(
            """
            CREATE TABLE contents (
                id BLOB NOT NULL PRIMARY KEY,
                content TEXT NOT NULL
            );
            CREATE TABLE deps (
                src BLOB NOT NULL,
                tgt BLOB NOT NULL,
                kind TEXT NOT NULL,
                row INT NOT NULL,
                commit_id BLOB
            );
            CREATE TABLE entities (
                id BLOB NOT NULL PRIMARY KEY,
                parent_id BLOB,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                start_byte INT NOT NULL,
                start_row INT NOT NULL,
                start_column INT NOT NULL,
                end_byte INT NOT NULL,
                end_row INT NOT NULL,
                end_column INT NOT NULL,
                comment_start_byte INT,
                comment_start_row INT,
                comment_start_column INT,
                comment_end_byte INT,
                comment_end_row INT,
                comment_end_column INT,
                content_id BLOB NOT NULL,
                simple_id BLOB NOT NULL
            );
            """
        )

        # Entities
        ent_placeholders = ",".join(["?"] * len(keep_entity_ids))
        ent_rows = cur.execute(
            f"""
            SELECT
              id, parent_id, name, kind,
              start_byte, start_row, start_column,
              end_byte, end_row, end_column,
              comment_start_byte, comment_start_row, comment_start_column,
              comment_end_byte, comment_end_row, comment_end_column,
              content_id, simple_id
            FROM entities
            WHERE id IN ({ent_placeholders})
            """,
            list(keep_entity_ids),
        ).fetchall()
        dst.executemany(
            """
            INSERT INTO entities (
              id, parent_id, name, kind,
              start_byte, start_row, start_column,
              end_byte, end_row, end_column,
              comment_start_byte, comment_start_row, comment_start_column,
              comment_end_byte, comment_end_row, comment_end_column,
              content_id, simple_id
            ) VALUES (
              ?, ?, ?, ?,
              ?, ?, ?,
              ?, ?, ?,
              ?, ?, ?,
              ?, ?, ?,
              ?, ?
            )
            """,
            ent_rows,
        )

        # Contents (only the ones referenced by kept entities)
        if keep_content_ids:
            c_placeholders = ",".join(["?"] * len(keep_content_ids))
            content_rows = cur.execute(
                f"SELECT id, content FROM contents WHERE id IN ({c_placeholders})",
                list(keep_content_ids),
            ).fetchall()
            dst.executemany("INSERT INTO contents (id, content) VALUES (?, ?)", content_rows)

        # Deps: keep only those where endpoints exist in the per-file DB.
        filtered_dep_rows = [r for r in dep_rows if r[0] in keep_entity_ids and r[1] in keep_entity_ids]
        dst.executemany("INSERT INTO deps (src, tgt, kind, row, commit_id) VALUES (?, ?, ?, ?, ?)", filtered_dep_rows)

        dst.commit()
        dst.close()

    con.close()


def _dv8_write_dependency_json(
    *,
    name: str,
    edges: Iterable[Tuple[str, str, str]],
    output_path: Path,
    sort_key: Optional[Callable[[str], Any]] = None,
) -> None:
    out, _variables = _dv8_build_dependency_json(name=name, edges=edges)
    if sort_key is not None:
        out = _dv8_reorder_dependency_json(out, sort_key=sort_key)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(out, indent=2), encoding="utf-8")

def _dv8_build_dependency_json(
    *,
    name: str,
    edges: Iterable[Tuple[str, str, str]],
) -> Tuple[Dict[str, Any], List[str]]:
    # edges: (src_name, tgt_name, dep_kind)
    variables: List[str] = []
    idx: Dict[str, int] = {}
    cell_map: Dict[Tuple[int, int], Dict[str, float]] = {}

    def ensure(v: str) -> int:
        if v in idx:
            return idx[v]
        i = len(variables)
        variables.append(v)
        idx[v] = i
        return i

    for src, tgt, kind in edges:
        s = ensure(src)
        t = ensure(tgt)
        key = (s, t)
        vals = cell_map.setdefault(key, {})
        vals[kind] = float(vals.get(kind, 0.0) + 1.0)

    cells = [{"src": s, "dest": t, "values": vals} for (s, t), vals in sorted(cell_map.items())]
    out = {"@schemaVersion": "1.0", "name": name, "variables": variables, "cells": cells}
    return out, variables


def _dv8_reorder_dependency_json(obj: Dict[str, Any], *, sort_key: Callable[[str], Any]) -> Dict[str, Any]:
    """
    Reorder `variables` and remap cell indices accordingly.

    DV8 uses the variables list order for matrix axes, and typically also for tree insertion order.
    """
    variables: List[str] = list(obj.get("variables") or [])
    if not variables:
        return obj

    order = sorted(range(len(variables)), key=lambda i: sort_key(variables[i]))
    old_to_new = {old: new for new, old in enumerate(order)}
    new_vars = [variables[i] for i in order]

    new_cells = []
    for cell in obj.get("cells") or []:
        new_cells.append(
            {
                "src": old_to_new[int(cell["src"])],
                "dest": old_to_new[int(cell["dest"])],
                "values": cell.get("values") or {},
            }
        )
    new_cells.sort(key=lambda c: (c["src"], c["dest"]))

    return {
        "@schemaVersion": obj.get("@schemaVersion", "1.0"),
        "name": obj.get("name", ""),
        "variables": new_vars,
        "cells": new_cells,
    }

def _ancestor_file_id(entities: Dict[bytes, DbEntity], entity_id: bytes, memo: Dict[bytes, Optional[bytes]]) -> Optional[bytes]:
    if entity_id in memo:
        return memo[entity_id]
    current = entities.get(entity_id)
    while current is not None:
        if current.kind == "File":
            memo[entity_id] = current.id
            return current.id
        if current.parent_id is None:
            memo[entity_id] = None
            return None
        current = entities.get(current.parent_id)
    memo[entity_id] = None
    return None


def export_dv8_file_level(
    *,
    db_path: Path,
    out_dir: Path,
    output_path: Optional[Path] = None,
    focus_prefix: Optional[str],
    include_root_py: bool,
    include_external_target_files: bool,
    include_self_edges: bool,
    align_handcount: bool,
    dv8_hierarchy: str,
) -> None:
    """
    Export a single DV8 dependency matrix at FILE level.

    Nodes: File entities
    Edges: aggregated dependency kinds between files (derived from entity-level deps)
    """
    con = _connect_ro(db_path)
    entities = _load_entities(con)
    cur = con.cursor()

    file_ids = {eid for eid, e in entities.items() if e.kind == "File"}
    file_name_by_id = {eid: entities[eid].name for eid in file_ids}

    def in_focus(file_name: str) -> bool:
        if align_handcount and file_name.endswith("/__init__.py"):
            return False
        if focus_prefix is None:
            return True
        if file_name.startswith(focus_prefix):
            return True
        if include_root_py and "/" not in file_name and file_name.endswith(".py"):
            return True
        return False

    focus_all = [name for name in file_name_by_id.values() if in_focus(name)]
    pkg_files = sorted([n for n in focus_all if "/" in n])
    root_files = sorted([n for n in focus_all if "/" not in n])
    focus_file_names = pkg_files + root_files

    # Map entity->file via parent chain.
    memo: Dict[bytes, Optional[bytes]] = {}

    dep_rows = cur.execute("SELECT src, tgt, kind FROM deps").fetchall()
    edges: List[Tuple[str, str, str]] = []
    for src_id, tgt_id, dep_kind in dep_rows:
        if src_id not in entities or tgt_id not in entities:
            continue

        src_file_id = _ancestor_file_id(entities, src_id, memo)
        tgt_file_id = _ancestor_file_id(entities, tgt_id, memo)
        if src_file_id is None or tgt_file_id is None:
            continue

        src_file_name = file_name_by_id.get(src_file_id)
        tgt_file_name = file_name_by_id.get(tgt_file_id)
        if not src_file_name or not tgt_file_name:
            continue

        if not in_focus(src_file_name):
            continue

        if not include_self_edges and src_file_id == tgt_file_id:
            continue

        if in_focus(tgt_file_name):
            if align_handcount:
                # Prefer Import-only file coupling when imports exist, but fall back to derived coupling
                # when no Import edges are present (common for StackGraphs on non-package folders).
                if dep_kind == "Import":
                    edges.append(
                        (_aligned_file_node(src_file_name, dv8_hierarchy), _aligned_file_node(tgt_file_name, dv8_hierarchy), dep_kind)
                    )
            else:
                edges.append(
                    (_aligned_file_node(src_file_name, dv8_hierarchy), _aligned_file_node(tgt_file_name, dv8_hierarchy), dep_kind)
                )
        else:
            if not include_external_target_files:
                continue
            if align_handcount:
                # Handcount DSM is internal-only.
                continue
            edges.append((_aligned_file_node(src_file_name, dv8_hierarchy), f"(External File) {tgt_file_name}", dep_kind))

    if align_handcount and not edges:
        # Fallback: derive file->file coupling from any cross-file edge when Import edges are missing.
        # We keep the original kind if it is in the core set; otherwise skip it.
        core_kinds = {"Import", "Extend", "Create", "Call", "Use"}
        for src_id, tgt_id, dep_kind in dep_rows:
            if dep_kind not in core_kinds:
                continue
            if src_id not in entities or tgt_id not in entities:
                continue
            src_file_id = _ancestor_file_id(entities, src_id, memo)
            tgt_file_id = _ancestor_file_id(entities, tgt_id, memo)
            if src_file_id is None or tgt_file_id is None:
                continue
            if not include_self_edges and src_file_id == tgt_file_id:
                continue
            src_file_name = file_name_by_id.get(src_file_id)
            tgt_file_name = file_name_by_id.get(tgt_file_id)
            if not src_file_name or not tgt_file_name:
                continue
            if not in_focus(src_file_name) or not in_focus(tgt_file_name):
                continue
            edges.append(
                (_aligned_file_node(src_file_name, dv8_hierarchy), _aligned_file_node(tgt_file_name, dv8_hierarchy), dep_kind)
            )

    if align_handcount:
        edges = sorted(set(edges))

    out_path = output_path or (out_dir / "dependencies.dv8-dependency.json")
    if align_handcount:
        # Always include all focus files as variables so DV8 shows the full file list,
        # even if some files have no edges.
        variables = [_aligned_file_node(f, dv8_hierarchy) for f in focus_file_names]
        index = {v: i for i, v in enumerate(variables)}
        cell_map: Dict[Tuple[int, int], Dict[str, float]] = {}
        for s, t, k in edges:
            if s not in index or t not in index:
                continue
            key = (index[s], index[t])
            values = cell_map.setdefault(key, {})
            values[k] = values.get(k, 0.0) + 1.0
        cells = [{"src": s, "dest": t, "values": v} for (s, t), v in sorted(cell_map.items())]
        out_path.write_text(
            json.dumps({"@schemaVersion": "1.0", "name": "dependencies (file-level)", "variables": variables, "cells": cells}, indent=2),
            encoding="utf-8",
        )
    else:
        _dv8_write_dependency_json(
            name="dependencies (file-level)",
            edges=edges,
            output_path=out_path,
            sort_key=_dv8_sort_key_for_hierarchy(dv8_hierarchy),
        )
    con.close()

def _display_name_with_file(
    entities: Dict[bytes, DbEntity],
    entity_id: bytes,
    *,
    file_id_memo: Dict[bytes, Optional[bytes]],
    file_name_by_id: Dict[bytes, str],
) -> Optional[str]:
    file_id = _ancestor_file_id(entities, entity_id, file_id_memo)
    if file_id is None:
        return None
    file_name = file_name_by_id.get(file_id)
    if not file_name:
        return None

    ent = entities[entity_id]
    if ent.kind == "File":
        return file_name

    # Make variables globally unique by prefixing with file path.
    return f"{file_name}::{_display_name(entities, entity_id)}"

def _handcount_file_node(file_name: str) -> str:
    # Canonical "file node" used by the handcount DSM to avoid DV8 showing a duplicate
    # "file (File)" item. This is an actual leaf variable that can have children.
    return f"{file_name}/module (Module)"

def _class_chain_names(entities: Dict[bytes, DbEntity], entity_id: bytes) -> List[str]:
    """
    Return the containing class chain (outer -> inner) for an entity.

    For a Class entity itself, this includes that class name.
    For a Method/Field, this includes its owning class(es).
    """
    chain: List[str] = []
    cur = entities.get(entity_id)
    while cur and cur.parent_id is not None and cur.parent_id in entities:
        parent = entities[cur.parent_id]
        if parent.kind == "Class":
            chain.append(parent.name)
        cur = parent
    chain.reverse()
    ent = entities.get(entity_id)
    if ent and ent.kind == "Class":
        chain.append(ent.name)
    # Guard against duplicate tagging that can create nested Class entities with the same name
    # (e.g., Class X extracted twice and one ends up parented by the other).
    deduped: List[str] = []
    for name in chain:
        if deduped and deduped[-1] == name:
            continue
        deduped.append(name)
    return deduped

def _structured_file_node(file_name: str) -> str:
    # Structured hierarchy: keep a file-local "self" leaf to attach file-level deps.
    return f"{file_name}/self (File)"

def _segments_from_class_chain(chain: List[str]) -> List[str]:
    """
    Convert a lexical nesting chain (Outer -> Inner -> ...) into structured path segments.

    Example:
      ["Outer", "Inner"] -> ["Outer", "inner_classes", "Inner"]
    """
    if not chain:
        return []
    segments: List[str] = [chain[0]]
    for name in chain[1:]:
        segments.extend(["inner_classes", name])
    return segments

def _build_structured_class_folder_map(
    *,
    entities: Dict[bytes, DbEntity],
    dep_rows: Sequence[Tuple[bytes, bytes, str]],
    file_id_memo: Dict[bytes, Optional[bytes]],
    file_name_by_id: Dict[bytes, str],
    in_focus_file: Callable[[str], bool],
) -> Dict[bytes, List[str]]:
    """
    Build a "class folder" map for structured DV8 hierarchy.

    Goal: represent local inheritance (Extend) as nesting in the tree:
      <file>/BaseClass/subclasses/DerivedClass/...

    Lexical inner classes (Class parented by Class) are represented as:
      <file>/Outer/inner_classes/Inner/...

    Precedence:
      lexical nesting > local inheritance

    Only nests along *single* local base-class chains (if multiple local bases exist, do not nest).
    """
    # Collect focus classes and their file association.
    class_ids: Set[bytes] = set()
    file_id_by_class: Dict[bytes, bytes] = {}
    for eid, ent in entities.items():
        if ent.kind != "Class":
            continue
        fid = _ancestor_file_id(entities, eid, file_id_memo)
        if fid is None:
            continue
        fname = file_name_by_id.get(fid)
        if not fname or not in_focus_file(fname):
            continue
        class_ids.add(eid)
        file_id_by_class[eid] = fid

    # Map derived class -> local base class (only if exactly one local base exists).
    local_bases: Dict[bytes, Set[bytes]] = {}
    for src_id, tgt_id, dep_kind in dep_rows:
        if dep_kind != "Extend":
            continue
        if src_id not in class_ids:
            continue
        if tgt_id not in entities or entities[tgt_id].kind != "Class":
            continue
        if src_id == tgt_id:
            continue
        src_fid = file_id_by_class.get(src_id)
        tgt_fid = file_id_by_class.get(tgt_id)
        if src_fid is None or tgt_fid is None or src_fid != tgt_fid:
            continue
        local_bases.setdefault(src_id, set()).add(tgt_id)

    local_base_of: Dict[bytes, Optional[bytes]] = {}
    for cid in class_ids:
        bases = local_bases.get(cid) or set()
        local_base_of[cid] = next(iter(bases)) if len(bases) == 1 else None

    memo: Dict[bytes, List[str]] = {}
    visiting: Set[bytes] = set()

    def folder_for_class(class_id: bytes) -> List[str]:
        if class_id in memo:
            return memo[class_id]
        ent = entities.get(class_id)
        if ent is None:
            return ["<unknown>"]
        if class_id in visiting:
            segs = [ent.name]
            memo[class_id] = segs
            return segs
        visiting.add(class_id)

        # (1) Lexical nesting: Class parented by another Class in the same file.
        parent_id = ent.parent_id
        if (
            parent_id is not None
            and parent_id in entities
            and entities[parent_id].kind == "Class"
            and file_id_by_class.get(parent_id) == file_id_by_class.get(class_id)
        ):
            segs = folder_for_class(parent_id) + ["inner_classes", ent.name]
            memo[class_id] = segs
            visiting.remove(class_id)
            return segs

        # (2) Local inheritance nesting.
        base_id = local_base_of.get(class_id)
        if base_id is not None:
            segs = folder_for_class(base_id) + ["subclasses", ent.name]
            memo[class_id] = segs
            visiting.remove(class_id)
            return segs

        # (3) Root class in this file.
        segs = [ent.name]
        memo[class_id] = segs
        visiting.remove(class_id)
        return segs

    for cid in class_ids:
        folder_for_class(cid)

    return memo

def _structured_name(
    entities: Dict[bytes, DbEntity],
    entity_id: bytes,
    *,
    file_id_memo: Dict[bytes, Optional[bytes]],
    file_name_by_id: Dict[bytes, str],
    class_folder_by_id: Optional[Dict[bytes, List[str]]] = None,
) -> Optional[str]:
    """
    Structured DV8 naming scheme (path-like):

      <file>/self (File)
      <file>/functions/<fn> (Function)
      <file>/<Class>/self (Class)
      <file>/<Class>/constructors/<ctor> (Constructor)
      <file>/<Class>/methods/<m> (Method)
      <file>/<Class>/fields/<f> (Field)
      <file>/<Base>/subclasses/<Derived>/... for local inheritance
      <file>/<Outer>/inner_classes/<Inner>/... for nested classes
    """
    file_id = _ancestor_file_id(entities, entity_id, file_id_memo)
    if file_id is None:
        return None
    file_name = file_name_by_id.get(file_id)
    if not file_name:
        return None

    ent = entities.get(entity_id)
    if ent is None:
        return None

    if ent.kind == "File":
        return _structured_file_node(file_name)

    owner_cls = ent if ent.kind == "Class" else _owner_class_entity(entities, entity_id)
    class_segments: List[str] = []
    if owner_cls is not None:
        if class_folder_by_id is not None and owner_cls.id in class_folder_by_id:
            class_segments = class_folder_by_id[owner_cls.id]
        else:
            chain = _class_chain_names(entities, owner_cls.id)
            class_segments = _segments_from_class_chain(chain)
    class_prefix = f"{file_name}/" + "/".join(class_segments) if class_segments else file_name

    if ent.kind == "Class":
        if not class_segments:
            return None
        return f"{class_prefix}/self (Class)"

    if ent.kind == "Field":
        if not class_segments:
            return None
        return f"{class_prefix}/fields/{ent.name} (Field)"

    if ent.kind == "Function":
        # Module-level functions (new: kind=Function for module-level, kind=Method for class methods)
        return f"{file_name}/functions/{ent.name} (Function)"

    if ent.kind == "Method":
        # Class methods only (module-level functions now use kind=Function)
        if not class_segments:
            # Fallback for old data where module functions were Method with parent=File
            return f"{file_name}/functions/{ent.name} (Function)"
        if ent.name in {"__init__", "__new__"}:
            return f"{class_prefix}/constructors/{ent.name} (Constructor)"
        return f"{class_prefix}/methods/{ent.name} (Method)"

    return f"{file_name}::{_display_name(entities, entity_id)}"

def _professor_name(
    entities: Dict[bytes, DbEntity],
    entity_id: bytes,
    *,
    file_id_memo: Dict[bytes, Optional[bytes]],
    file_name_by_id: Dict[bytes, str],
    class_folder_by_id: Optional[Dict[bytes, List[str]]] = None,
) -> Optional[str]:
    # Deprecated alias (kept for backwards compatibility).
    return _structured_name(
        entities,
        entity_id,
        file_id_memo=file_id_memo,
        file_name_by_id=file_name_by_id,
        class_folder_by_id=class_folder_by_id,
    )

def _build_local_base_dotted_map(
    *,
    entities: Dict[bytes, DbEntity],
    dep_rows: Sequence[Tuple[bytes, bytes, str]],
    file_id_memo: Dict[bytes, Optional[bytes]],
    file_name_by_id: Dict[bytes, str],
    in_focus_file: Callable[[str], bool],
) -> Dict[bytes, str]:
    """
    For each Class in focus, if it has exactly one local base class (Extend) in the same file,
    map derived_class_id -> base_class_dotted_name.
    """
    class_ids: Set[bytes] = set()
    file_id_by_class: Dict[bytes, bytes] = {}
    for eid, ent in entities.items():
        if ent.kind != "Class":
            continue
        fid = _ancestor_file_id(entities, eid, file_id_memo)
        if fid is None:
            continue
        fname = file_name_by_id.get(fid)
        if not fname or not in_focus_file(fname):
            continue
        class_ids.add(eid)
        file_id_by_class[eid] = fid

    local_bases: Dict[bytes, Set[bytes]] = {}
    for src_id, tgt_id, dep_kind in dep_rows:
        if dep_kind != "Extend":
            continue
        if src_id not in class_ids:
            continue
        if tgt_id not in entities or entities[tgt_id].kind != "Class":
            continue
        src_fid = file_id_by_class.get(src_id)
        tgt_fid = file_id_by_class.get(tgt_id)
        if src_fid is None or tgt_fid is None or src_fid != tgt_fid:
            continue
        local_bases.setdefault(src_id, set()).add(tgt_id)

    base_dotted: Dict[bytes, str] = {}
    for derived_id, bases in local_bases.items():
        if len(bases) != 1:
            continue
        base_id = next(iter(bases))
        chain = _class_chain_names(entities, base_id)
        base_dotted[derived_id] = ".".join(chain) if chain else entities[base_id].name
    return base_dotted

def _flat_name(
    entities: Dict[bytes, DbEntity],
    entity_id: bytes,
    *,
    file_id_memo: Dict[bytes, Optional[bytes]],
    file_name_by_id: Dict[bytes, str],
    local_base_dotted_by_class_id: Optional[Dict[bytes, str]] = None,
) -> Optional[str]:
    """
    Flat DV8 naming scheme:

      <file>/self (File)
      <file>/-self <Class> (Class)
      <file>/+CONSTRUCTORS/<Class>/<ctor> (Constructor)
      <file>/+METHODS/<Class>/<m> (Method)
      <file>/+FIELDS/<Class>/<f> (Field)
      <file>/+FUNCTIONS/<fn> (Function)

    Slight nesting for subclasses (same file):
      <file>/+SUBCLASSES/<Base>/-self <Derived> (Class)
    """
    file_id = _ancestor_file_id(entities, entity_id, file_id_memo)
    if file_id is None:
        return None
    file_name = file_name_by_id.get(file_id)
    if not file_name:
        return None

    ent = entities.get(entity_id)
    if ent is None:
        return None

    if ent.kind == "File":
        return _structured_file_node(file_name)

    cls = ent if ent.kind == "Class" else _owner_class_entity(entities, entity_id)
    cls_dotted: Optional[str] = None
    if cls is not None:
        chain = _class_chain_names(entities, cls.id)
        cls_dotted = ".".join(chain) if chain else cls.name

    if ent.kind == "Class":
        if cls_dotted is None:
            return None
        if local_base_dotted_by_class_id and ent.id in local_base_dotted_by_class_id:
            base = local_base_dotted_by_class_id[ent.id]
            return f"{file_name}/+SUBCLASSES/{base}/-self {cls_dotted} (Class)"
        return f"{file_name}/-self {cls_dotted} (Class)"

    if ent.kind == "Field":
        if cls_dotted is None:
            return None
        return f"{file_name}/+FIELDS/{cls_dotted}/{ent.name} (Field)"

    if ent.kind == "Method":
        # Module-level functions are stored as kind=Method with parent=File.
        if cls_dotted is None:
            return f"{file_name}/+FUNCTIONS/{ent.name} (Function)"
        if ent.name in {"__init__", "__new__"}:
            return f"{file_name}/+CONSTRUCTORS/{cls_dotted}/{ent.name} (Constructor)"
        return f"{file_name}/+METHODS/{cls_dotted}/{ent.name} (Method)"

    return f"{file_name}::{_display_name(entities, entity_id)}"

def _aligned_file_node(file_name: str, dv8_hierarchy: str) -> str:
    if dv8_hierarchy == "handcount":
        return _handcount_file_node(file_name)
    return _structured_file_node(file_name)

def _aligned_name(
    entities: Dict[bytes, DbEntity],
    entity_id: bytes,
    *,
    dv8_hierarchy: str,
    file_id_memo: Dict[bytes, Optional[bytes]],
    file_name_by_id: Dict[bytes, str],
    class_folder_by_id: Optional[Dict[bytes, List[str]]] = None,
    local_base_dotted_by_class_id: Optional[Dict[bytes, str]] = None,
) -> Optional[str]:
    if dv8_hierarchy == "handcount":
        return _handcount_name(entities, entity_id, file_id_memo=file_id_memo, file_name_by_id=file_name_by_id)
    if dv8_hierarchy == "flat":
        return _flat_name(
            entities,
            entity_id,
            file_id_memo=file_id_memo,
            file_name_by_id=file_name_by_id,
            local_base_dotted_by_class_id=local_base_dotted_by_class_id,
        )
    return _structured_name(
        entities,
        entity_id,
        file_id_memo=file_id_memo,
        file_name_by_id=file_name_by_id,
        class_folder_by_id=class_folder_by_id,
    )

def _handcount_name(
    entities: Dict[bytes, DbEntity],
    entity_id: bytes,
    *,
    file_id_memo: Dict[bytes, Optional[bytes]],
    file_name_by_id: Dict[bytes, str],
) -> Optional[str]:
    """
    Handcount-compatible DV8 naming scheme (path-like):

      <file>/module (Module)
      <file>/FUNCTIONS/<fn> (Function)
      <file>/CLASSES/<Class> (Class)
      <file>/CLASSES/<Class>/CONSTRUCTORS/<ctor> (Constructor)
      <file>/CLASSES/<Class>/METHODS/<m> (Method)
      <file>/CLASSES/<Class>/FIELDS/<f> (Field)

    This makes DV8 show the hierarchy without requiring a clustering JSON.
    """
    file_id = _ancestor_file_id(entities, entity_id, file_id_memo)
    if file_id is None:
        return None
    file_name = file_name_by_id.get(file_id)
    if not file_name:
        return None

    ent = entities.get(entity_id)
    if ent is None:
        return None

    if ent.kind == "File":
        return _handcount_file_node(file_name)

    # Walk up to find class owner (if any).
    cls = _owner_class_entity(entities, entity_id)
    cls_dotted: Optional[str] = None
    if cls is not None:
        chain = _class_chain_names(entities, cls.id)
        if chain:
            cls_dotted = ".".join(chain)

    if ent.kind == "Class":
        chain = _class_chain_names(entities, entity_id)
        if not chain:
            return None
        return f"{file_name}/CLASSES/{'.'.join(chain)} (Class)"

    if ent.kind == "Field":
        if cls is None or cls_dotted is None:
            return None
        return f"{file_name}/CLASSES/{cls_dotted}/FIELDS/{ent.name} (Field)"

    if ent.kind == "Method":
        # Module-level functions are stored as kind=Method with parent=File.
        if cls is None:
            return f"{file_name}/FUNCTIONS/{ent.name} (Function)"
        if cls_dotted is None:
            return None
        if ent.name in {"__init__", "__new__"}:
            return f"{file_name}/CLASSES/{cls_dotted}/CONSTRUCTORS/{ent.name} (Constructor)"
        return f"{file_name}/CLASSES/{cls_dotted}/METHODS/{ent.name} (Method)"

    # Fallback: keep legacy naming for unexpected kinds.
    return f"{file_name}::{_display_name(entities, entity_id)}"

def _owner_class_entity(entities: Dict[bytes, DbEntity], entity_id: bytes) -> Optional[DbEntity]:
    """
    Return the owning Class entity for an entity (Method/Field/etc), if any.

    For Python, module-level functions are stored as kind=Method with parent=File;
    those should return None here.
    """
    cur = entities.get(entity_id)
    if cur is None:
        return None
    while cur.parent_id is not None and cur.parent_id in entities:
        cur = entities[cur.parent_id]
        if cur.kind == "Class":
            return cur
        if cur.kind == "File":
            return None
    return None

def _is_nested_method(entities: Dict[bytes, DbEntity], entity_id: bytes) -> bool:
    """
    Return True if this Method/Function entity is nested inside another Method/Function (i.e., a local helper
    function defined inside a method/function body).

    Why (handcount alignment): the handcount ground truth treats dependencies inside nested
    helper functions as part of the *enclosing* method. Keeping nested Method/Function entities
    produces systematic "extra" edges (e.g. `apply` + `filter` both using the same fields).
    """
    ent = entities.get(entity_id)
    if ent is None or ent.kind not in {"Method", "Function"}:
        return False
    cur = ent
    seen: Set[bytes] = set()
    while cur.parent_id is not None and cur.parent_id in entities:
        if cur.id in seen:
            return False
        seen.add(cur.id)
        parent = entities[cur.parent_id]
        if parent.kind in {"Method", "Function"}:
            # Guard against duplicate tagging artifacts where a method ends up parented by a
            # method with the same name (e.g. two Method entities for `with_mask`, one nested
            # under the other). Treat these as duplicates, not real nested helper functions.
            if parent.name == ent.name:
                cur = parent
                continue
            return True
        if parent.kind in {"Class", "File"}:
            return False
        cur = parent
    return False

def _handcount_var_sort_key(var: str) -> Tuple[int, str, int, str, str]:
    """
    Deterministic variable ordering for handcount-aligned DSMs:
    - Package files (e.g. tts/...) before root files (e.g. main.py)
    - Within a file: module, functions, classes, constructors, methods, fields
    - Then by class/member name lexicographically
    """
    # External nodes (should be absent in align mode) go last.
    if var.startswith("(External "):
        return (9, var, 9, "", var)

    # Extract file path: substring up to first ".py"
    file_path = var
    if ".py/" in var:
        file_path = var.split(".py/", 1)[0] + ".py"
    elif var.endswith(".py"):
        file_path = var

    file_group = 0 if "/" in file_path else 1  # pkg-first

    # Type rank within file.
    if var.endswith("/module (Module)"):
        type_rank = 0
    elif "/FUNCTIONS/" in var:
        type_rank = 1
    elif "/CLASSES/" in var and var.endswith(" (Class)") and "/FIELDS/" not in var and "/METHODS/" not in var and "/CONSTRUCTORS/" not in var:
        type_rank = 2
    elif "/CONSTRUCTORS/" in var:
        type_rank = 3
    elif "/METHODS/" in var:
        type_rank = 4
    elif "/FIELDS/" in var:
        type_rank = 5
    else:
        type_rank = 8

    class_name = ""
    member_name = ""
    if "/CLASSES/" in var:
        after = var.split("/CLASSES/", 1)[1]
        class_name = after.split("/", 1)[0].replace(" (Class)", "")

    # member name: last path segment before " ("
    leaf = var.rsplit("/", 1)[-1]
    member_name = leaf.split(" (", 1)[0]

    return (file_group, file_path, type_rank, class_name, member_name)

def _professor_var_sort_key(var: str) -> Tuple[int, str, int, str, str]:
    """
    Deterministic ordering for professor-style hierarchy (deprecated).

    - Package files before root files
    - Within a file: self, functions, classes (self), constructors, methods, fields
    - Then lexicographically by class/member
    """
    return _structured_var_sort_key(var)


def _structured_var_sort_key(var: str) -> Tuple[int, str, int, str, str]:
    """
    Deterministic ordering for structured DV8 hierarchy.

    - Package files before root files
    - Within a file: self(File), functions, self(Class), constructors, methods, fields
    - Then lexicographically by class/member
    """
    if var.startswith("(External "):
        return (9, var, 9, "", var)

    file_path = var
    if ".py/" in var:
        file_path = var.split(".py/", 1)[0] + ".py"
    elif var.endswith(".py"):
        file_path = var

    file_group = 0 if "/" in file_path else 1

    type_rank = 8
    if var.endswith("/self (File)"):
        type_rank = 0
    elif "/functions/" in var:
        type_rank = 1
    elif var.endswith("/self (Class)"):
        type_rank = 2
    elif "/constructors/" in var:
        type_rank = 3
    elif "/methods/" in var:
        type_rank = 4
    elif "/fields/" in var:
        type_rank = 5

    class_chain = ""
    if ".py/" in var:
        rest = var.split(".py/", 1)[1]
        parts = rest.split("/")
        if parts:
            if parts[0] not in {"functions", "self (File)"}:
                if len(parts) >= 2:
                    class_chain = "/".join(parts[:-1])

    leaf = var.rsplit("/", 1)[-1]
    member_name = leaf.split(" (", 1)[0]

    return (file_group, file_path, type_rank, class_chain, member_name)


def _flat_var_sort_key(var: str) -> Tuple[int, str, int, str, str]:
    """
    Deterministic ordering for flat DV8 hierarchy.

    - Package files before root files
    - Within a file: self(File), -self(Class), +CONSTRUCTORS, +FIELDS, +METHODS, +FUNCTIONS, +SUBCLASSES
    """
    if var.startswith("(External "):
        return (9, var, 9, "", var)

    file_path = var
    if ".py/" in var:
        file_path = var.split(".py/", 1)[0] + ".py"
    elif var.endswith(".py"):
        file_path = var

    file_group = 0 if "/" in file_path else 1

    type_rank = 8
    if var.endswith("/self (File)"):
        type_rank = 0
    elif "/-self " in var:
        type_rank = 1
    elif "/+CONSTRUCTORS/" in var:
        type_rank = 2
    elif "/+FIELDS/" in var:
        type_rank = 3
    elif "/+METHODS/" in var:
        type_rank = 4
    elif "/+FUNCTIONS/" in var:
        type_rank = 5
    elif "/+SUBCLASSES/" in var:
        type_rank = 6

    class_chain = ""
    if ".py/" in var:
        rest = var.split(".py/", 1)[1]
        parts = rest.split("/")
        if len(parts) >= 2:
            class_chain = "/".join(parts[:-1])

    leaf = var.rsplit("/", 1)[-1]
    member_name = leaf.split(" (", 1)[0]
    return (file_group, file_path, type_rank, class_chain, member_name)


def _dv8_sort_key_for_hierarchy(dv8_hierarchy: str) -> Callable[[str], Any]:
    if dv8_hierarchy == "handcount":
        return _handcount_var_sort_key
    if dv8_hierarchy == "flat":
        return _flat_var_sort_key
    return _structured_var_sort_key


def export_dv8_full_project(
    *,
    db_path: Path,
    out_dir: Path,
    output_path: Optional[Path] = None,
    focus_prefix: Optional[str],
    include_root_py: bool,
    include_external_targets: bool,
    include_external_target_files: bool,
    include_self_edges: bool,
    align_handcount: bool,
    dv8_hierarchy: str,
) -> None:
    """
    Export a single "full" DV8 dependency matrix that supports drill-down in DV8.

    The hierarchy is encoded in the variable names (slash-delimited), so a separate
    DV8 clustering JSON is intentionally not required.
    """
    con = _connect_ro(db_path)
    entities = _load_entities(con)
    cur = con.cursor()

    file_ids = {eid for eid, e in entities.items() if e.kind == "File"}
    file_name_by_id = {eid: entities[eid].name for eid in file_ids}

    def in_focus(file_name: str) -> bool:
        if align_handcount and file_name.endswith("/__init__.py"):
            return False
        if focus_prefix is None:
            return True
        if file_name.startswith(focus_prefix):
            return True
        if include_root_py and "/" not in file_name and file_name.endswith(".py"):
            return True
        return False

    file_id_memo: Dict[bytes, Optional[bytes]] = {}

    focus_all = [name for name in file_name_by_id.values() if in_focus(name)]
    # Ordering: show package files (e.g. tts/...) before root files (e.g. main.py).
    pkg_files = sorted([n for n in focus_all if "/" in n])
    root_files = sorted([n for n in focus_all if "/" not in n])
    focus_file_names = pkg_files + root_files

    dep_rows = cur.execute("SELECT src, tgt, kind FROM deps").fetchall()
    core_kinds = {"Import", "Extend", "Create", "Call", "Use"}

    # (1) File-level edges (file -> file) for overview within the same DSM.
    file_level_edges: List[Tuple[str, str, str]] = []
    for src_id, tgt_id, dep_kind in dep_rows:
        if src_id not in entities or tgt_id not in entities:
            continue
        if align_handcount and dep_kind not in core_kinds:
            continue
        if align_handcount and dep_kind != "Import":
            continue

        src_file_id = _ancestor_file_id(entities, src_id, file_id_memo)
        tgt_file_id = _ancestor_file_id(entities, tgt_id, file_id_memo)
        if src_file_id is None or tgt_file_id is None:
            continue

        src_file_name = file_name_by_id.get(src_file_id)
        tgt_file_name = file_name_by_id.get(tgt_file_id)
        if not src_file_name or not tgt_file_name:
            continue

        if not in_focus(src_file_name):
            continue

        if not include_self_edges and src_file_id == tgt_file_id:
            continue

        src_node = _aligned_file_node(src_file_name, dv8_hierarchy)
        if in_focus(tgt_file_name):
            tgt_node = _aligned_file_node(tgt_file_name, dv8_hierarchy)
            file_level_edges.append((src_node, tgt_node, dep_kind))
        else:
            if not include_external_target_files:
                continue
            if align_handcount:
                # Handcount DSM is internal-only.
                continue
            file_level_edges.append((src_node, f"(External File) {tgt_file_name}", dep_kind))

    # (2) Entity-level edges for drill-down.
    # Keep focus entities: file/class/method/function/field entities that belong to focus files.
    focus_entity_ids: Set[bytes] = set()
    for eid, ent in entities.items():
        if ent.kind not in {"File", "Class", "Method", "Function", "Field"}:
            continue
        file_id = _ancestor_file_id(entities, eid, file_id_memo)
        if file_id is None:
            continue
        file_name = file_name_by_id.get(file_id)
        if not file_name or not in_focus(file_name):
            continue
        focus_entity_ids.add(eid)

    full_edges: List[Tuple[str, str, str]] = []
    full_edges.extend(file_level_edges)

    structured_class_folders: Optional[Dict[bytes, List[str]]] = None
    local_base_dotted_by_class_id: Optional[Dict[bytes, str]] = None
    if dv8_hierarchy == "structured":
        structured_class_folders = _build_structured_class_folder_map(
            entities=entities,
            dep_rows=dep_rows,
            file_id_memo=file_id_memo,
            file_name_by_id=file_name_by_id,
            in_focus_file=in_focus,
        )
    elif dv8_hierarchy == "flat":
        local_base_dotted_by_class_id = _build_local_base_dotted_map(
            entities=entities,
            dep_rows=dep_rows,
            file_id_memo=file_id_memo,
            file_name_by_id=file_name_by_id,
            in_focus_file=in_focus,
        )

    if align_handcount:
        # Handcount DSM is internal-only.
        include_external_targets = False

    for src_id, tgt_id, dep_kind in dep_rows:
        if src_id not in focus_entity_ids:
            continue
        if tgt_id not in entities:
            continue

        if align_handcount:
            if dep_kind not in core_kinds:
                continue
            src_kind = entities[src_id].kind
            tgt_kind = entities[tgt_id].kind
            # Collapse Python nested helper functions: ignore nested Method/Function entities.
            # The handcount ground truth attributes nested-function dependencies to the
            # enclosing method/function, so keeping them here mostly creates duplicate "extra" edges.
            if src_kind in {"Method", "Function"} and _is_nested_method(entities, src_id):
                continue
            if tgt_kind in {"Method", "Function"} and _is_nested_method(entities, tgt_id):
                continue
            # Strict handcount schema:
            # - Import: File -> File
            # - Extend: Class -> Class
            # - Create: Method -> Class
            # - Call: Method -> Method
            # - Use: Method -> Field
            if dep_kind == "Import" and not (src_kind == "File" and tgt_kind == "File"):
                continue
            if dep_kind == "Extend" and not (src_kind == "Class" and tgt_kind == "Class"):
                continue
            if dep_kind == "Create" and not (src_kind in {"Method", "Function"} and tgt_kind == "Class"):
                continue
            if dep_kind == "Call" and not (src_kind in {"Method", "Function"} and tgt_kind in {"Method", "Function"}):
                continue
            if dep_kind == "Use" and not (src_kind in {"Method", "Function"} and tgt_kind == "Field"):
                continue
            if dep_kind == "Use" and _owner_class_entity(entities, src_id) is None:
                # Handcount rules treat Use as "method/constructor uses its own fields" (self.field),
                # not arbitrary cross-object attribute reads (e.g. `ticket.ticket_id` in `main()`).
                continue

            # Avoid spurious constructor calls: handcount treats constructor invocations as Create edges,
            # and keeps Call edges for `super().__init__` only (i.e., constructor -> constructor on base).
            if dep_kind == "Call" and tgt_kind == "Method" and entities[tgt_id].name in {"__init__", "__new__"}:
                if src_kind not in {"Method", "Function"} or (src_kind == "Method" and entities[src_id].name not in {"__init__", "__new__"}):
                    continue

            # Handcount rules treat "type coupling" (Method/Function -> Class Use) as noise by default,
            # and Field -> Method edges as directionally wrong for "uses".
            # Some resolvers also model `super().__init__` as Method -> Class Call; drop those too.
            if src_kind in {"Method", "Function"} and tgt_kind == "Class" and dep_kind != "Create":
                continue
            if src_kind == "Field" and tgt_kind == "Method":
                continue
            if src_kind == "Class" and tgt_kind == "Class" and dep_kind == "Use":
                # Prefer the explicit inheritance edge (Extend) and avoid double-counting via type-use.
                continue

        src_name = _aligned_name(
            entities,
            src_id,
            dv8_hierarchy=dv8_hierarchy,
            file_id_memo=file_id_memo,
            file_name_by_id=file_name_by_id,
            class_folder_by_id=structured_class_folders,
            local_base_dotted_by_class_id=local_base_dotted_by_class_id,
        )
        if not src_name:
            continue

        if tgt_id in focus_entity_ids:
            tgt_name = _aligned_name(
                entities,
                tgt_id,
                dv8_hierarchy=dv8_hierarchy,
                file_id_memo=file_id_memo,
                file_name_by_id=file_name_by_id,
                class_folder_by_id=structured_class_folders,
                local_base_dotted_by_class_id=local_base_dotted_by_class_id,
            )
            if not tgt_name:
                continue
        else:
            if not include_external_targets:
                continue
            tgt_ent = entities[tgt_id]
            # External targets: keep them as flat nodes in an "External" folder.
            tgt_name = f"(External {tgt_ent.kind}) {_display_name(entities, tgt_id)}"

        full_edges.append((src_name, tgt_name, dep_kind))

    if align_handcount:
        # Handcount counts unique edges, not call-sites.
        full_edges = sorted(set(full_edges))

    dep_path = output_path or (out_dir / "dependencies.full.dv8-dependency.json")
    _dv8_write_dependency_json(
        name="dependencies (full)",
        edges=full_edges,
        output_path=dep_path,
        sort_key=_dv8_sort_key_for_hierarchy(dv8_hierarchy),
    )
    con.close()


def _summarize_db(db_path: Path) -> Dict[str, Any]:
    con = _connect_ro(db_path)
    cur = con.cursor()

    entities_total = cur.execute("SELECT COUNT(*) FROM entities").fetchone()[0]
    deps_total = cur.execute("SELECT COUNT(*) FROM deps").fetchone()[0]
    files_total = cur.execute("SELECT COUNT(*) FROM entities WHERE kind = 'File'").fetchone()[0]
    classes_total = cur.execute("SELECT COUNT(*) FROM entities WHERE kind = 'Class'").fetchone()[0]
    methods_total = cur.execute("SELECT COUNT(*) FROM entities WHERE kind = 'Method'").fetchone()[0]
    fields_total = cur.execute("SELECT COUNT(*) FROM entities WHERE kind = 'Field'").fetchone()[0]

    deps_by_kind = {
        kind: count
        for kind, count in cur.execute("SELECT kind, COUNT(*) FROM deps GROUP BY kind ORDER BY kind").fetchall()
    }

    breakdown_rows = cur.execute(
        """
        SELECT e_src.kind, e_tgt.kind, d.kind, COUNT(*) as count
        FROM deps d
        JOIN entities e_src ON d.src = e_src.id
        JOIN entities e_tgt ON d.tgt = e_tgt.id
        GROUP BY e_src.kind, e_tgt.kind, d.kind
        ORDER BY count DESC
        """
    ).fetchall()
    breakdown = [
        {"src_kind": s, "tgt_kind": t, "dep_kind": k, "count": c} for (s, t, k, c) in breakdown_rows
    ]

    con.close()
    return {
        "entities_total": entities_total,
        "deps_total": deps_total,
        "files_total": files_total,
        "classes_total": classes_total,
        "methods_total": methods_total,
        "fields_total": fields_total,
        "deps_by_kind": deps_by_kind,
        "breakdown": breakdown,
    }


def _summarize_dv8_dir(dv8_dir: Path) -> Dict[str, Any]:
    totals: Dict[str, int] = {}
    per_file: Dict[str, Dict[str, int]] = {}
    if not dv8_dir.exists():
        return {"totals": totals, "per_file": per_file}

    for path in sorted(dv8_dir.glob("*.dv8-dependency.json")):
        obj = json.loads(path.read_text(encoding="utf-8"))
        file_counts: Dict[str, int] = {}
        for cell in obj.get("cells", []):
            for kind, val in (cell.get("values") or {}).items():
                n = int(val)
                totals[kind] = totals.get(kind, 0) + n
                file_counts[kind] = file_counts.get(kind, 0) + n
        per_file[path.name] = file_counts

    return {"totals": totals, "per_file": per_file}


def export_dv8_per_file(
    *,
    db_path: Path,
    out_dir: Path,
    include_external_targets: bool,
    include_incoming_edges: bool,
    only_py: bool,
    focus_prefix: Optional[str],
    include_root_py: bool,
    write_clustering: bool,
    align_handcount: bool,
    dv8_hierarchy: str,
) -> None:
    con = _connect_ro(db_path)
    entities = _load_entities(con)
    cur = con.cursor()

    file_rows = cur.execute("SELECT id, name FROM entities WHERE kind = 'File' ORDER BY name").fetchall()
    core_kinds = {"Import", "Extend", "Create", "Call", "Use"}
    for file_id, file_name in file_rows:
        if align_handcount and file_name.endswith("/__init__.py"):
            continue
        if only_py and not file_name.endswith(".py"):
            continue
        if focus_prefix is not None:
            if file_name.startswith(focus_prefix):
                pass
            elif include_root_py and "/" not in file_name and file_name.endswith(".py"):
                pass
            else:
                continue

        ids = _descendants(con, file_id)
        placeholders = ",".join(["?"] * len(ids))
        if include_incoming_edges:
            dep_rows = cur.execute(
                f"""
                SELECT d.src, d.tgt, d.kind
                FROM deps d
                WHERE d.src IN ({placeholders}) OR d.tgt IN ({placeholders})
                """,
                list(ids) + list(ids),
            ).fetchall()
        else:
            dep_rows = cur.execute(
                f"""
                SELECT d.src, d.tgt, d.kind
                FROM deps d
                WHERE d.src IN ({placeholders})
                """,
                list(ids),
            ).fetchall()

        edges: List[Tuple[str, str, str]] = []
        if align_handcount:
            file_id_memo: Dict[bytes, Optional[bytes]] = {}
            file_name_by_id = {eid: e.name for eid, e in entities.items() if e.kind == "File"}
            class_folder_by_id: Optional[Dict[bytes, List[str]]] = None
            local_base_dotted_by_class_id: Optional[Dict[bytes, str]] = None
            if dv8_hierarchy == "structured":
                class_folder_by_id = _build_structured_class_folder_map(
                    entities=entities,
                    dep_rows=dep_rows,
                    file_id_memo=file_id_memo,
                    file_name_by_id=file_name_by_id,
                    in_focus_file=lambda f, fn=file_name: f == fn,
                )
            elif dv8_hierarchy == "flat":
                local_base_dotted_by_class_id = _build_local_base_dotted_map(
                    entities=entities,
                    dep_rows=dep_rows,
                    file_id_memo=file_id_memo,
                    file_name_by_id=file_name_by_id,
                    in_focus_file=lambda f, fn=file_name: f == fn,
                )

        for src_id, tgt_id, dep_kind in dep_rows:
            if src_id not in entities or tgt_id not in entities:
                continue

            src_in_file = src_id in ids
            tgt_in_file = tgt_id in ids

            tgt_ent = entities[tgt_id]
            if align_handcount:
                if dep_kind not in core_kinds:
                    continue
                src_ent = entities[src_id]
                # Strict handcount schema by dep kind.
                if dep_kind == "Import" and not (src_ent.kind == "File" and tgt_ent.kind == "File"):
                    continue
                if dep_kind == "Extend" and not (src_ent.kind == "Class" and tgt_ent.kind == "Class"):
                    continue
                if dep_kind == "Create" and not (src_ent.kind == "Method" and tgt_ent.kind == "Class"):
                    continue
                if dep_kind == "Call" and not (src_ent.kind == "Method" and tgt_ent.kind == "Method"):
                    continue
                if dep_kind == "Use" and not (src_ent.kind == "Method" and tgt_ent.kind == "Field"):
                    continue

                if dep_kind == "Call" and tgt_ent.kind == "Method" and tgt_ent.name in {"__init__", "__new__"}:
                    if src_ent.name not in {"__init__", "__new__"}:
                        continue

                # Drop Method->Class Use (type coupling) and Field->Method edges for handcount alignment.
                if src_ent.kind == "Method" and tgt_ent.kind == "Class" and dep_kind != "Create":
                    continue
                if src_ent.kind == "Field" and tgt_ent.kind == "Method":
                    continue
                if src_ent.kind == "Class" and tgt_ent.kind == "Class" and dep_kind == "Use":
                    continue

            if align_handcount:
                if src_in_file:
                    src_name = _aligned_name(
                        entities,
                        src_id,
                        dv8_hierarchy=dv8_hierarchy,
                        file_id_memo=file_id_memo,
                        file_name_by_id=file_name_by_id,
                        class_folder_by_id=class_folder_by_id,
                        local_base_dotted_by_class_id=local_base_dotted_by_class_id,
                    )
                else:
                    if not include_external_targets:
                        continue
                    src_ent = entities[src_id]
                    src_name = f"(External {src_ent.kind}) {_display_name(entities, src_id)}"
            else:
                src_name = _display_name(entities, src_id)
            if tgt_in_file:
                if align_handcount:
                    tgt_name = _aligned_name(
                        entities,
                        tgt_id,
                        dv8_hierarchy=dv8_hierarchy,
                        file_id_memo=file_id_memo,
                        file_name_by_id=file_name_by_id,
                        class_folder_by_id=class_folder_by_id,
                        local_base_dotted_by_class_id=local_base_dotted_by_class_id,
                    )
                else:
                    tgt_name = _display_name(entities, tgt_id)
            else:
                if not include_external_targets:
                    continue
                tgt_name = f"(External {tgt_ent.kind}) {_display_name(entities, tgt_id)}"

            if not src_name or not tgt_name:
                continue
            edges.append((src_name, tgt_name, dep_kind))

        if align_handcount:
            edges = sorted(set(edges))

        out_path = out_dir / "dv8_deps" / f"{Path(file_name).stem}.dv8-dependency.json"
        dv8_obj, variables = _dv8_build_dependency_json(name=Path(file_name).name, edges=edges)
        if align_handcount:
            dv8_obj = _dv8_reorder_dependency_json(dv8_obj, sort_key=_dv8_sort_key_for_hierarchy(dv8_hierarchy))
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(json.dumps(dv8_obj, indent=2), encoding="utf-8")

        if write_clustering and not align_handcount:
            clustering = build_class_folder_clustering(
                db_entities=entities,
                file_id=file_id,
                file_name=file_name,
                variables=set(variables),
            )
            clustering_path = out_dir / "dv8_deps" / f"{Path(file_name).stem}.dv8-clustering.json"
            clustering_path.write_text(json.dumps(clustering, indent=2), encoding="utf-8")

    con.close()

def build_class_folder_clustering(
    *,
    db_entities: Dict[bytes, DbEntity],
    file_id: bytes,
    file_name: str,
    variables: Set[str],
) -> Dict[str, Any]:
    """
    Create a DV8 clustering structure for a single file dependency matrix.

    Desired view:
      <Class>
        self
        constructor
        methods
        fields
      External
      Module-level
    """

    def item(name: str) -> Dict[str, str]:
        return {"@type": "item", "name": name}

    def group(name: str, nested: List[Dict[str, Any]]) -> Dict[str, Any]:
        return {"@type": "group", "name": name, "nested": nested}

    # Find classes in this file (by content_id match, then parent chain includes file).
    file_ent = db_entities[file_id]
    file_content_id = file_ent.content_id

    # Collect classes that belong to this file.
    classes = [e for e in db_entities.values() if e.kind == "Class" and e.content_id == file_content_id]
    classes.sort(key=lambda e: e.name)

    assigned: Set[str] = set()
    structure: List[Dict[str, Any]] = []

    for cls in classes:
        cls_items: List[Dict[str, Any]] = []

        # self: include the class node itself if present in the matrix variables
        self_items: List[Dict[str, Any]] = []
        if cls.name in variables:
            self_items.append(item(cls.name))
            assigned.add(cls.name)
        if self_items:
            cls_items.append(group("self", self_items))

        # children (methods/fields) under this class (parent_id == class id)
        child_methods = [e for e in db_entities.values() if e.kind == "Method" and e.parent_id == cls.id]
        child_fields = [e for e in db_entities.values() if e.kind == "Field" and e.parent_id == cls.id]
        child_methods.sort(key=lambda e: e.name)
        child_fields.sort(key=lambda e: e.name)

        ctor_names: List[str] = []
        method_names: List[str] = []
        for m in child_methods:
            name = f"{cls.name}.{m.name}"
            if name not in variables:
                continue
            if m.name in {"__init__", "__new__"}:
                ctor_names.append(name)
            else:
                method_names.append(name)

        field_names = [f"{cls.name}.{f.name}" for f in child_fields if f"{cls.name}.{f.name}" in variables]

        if ctor_names:
            cls_items.append(group("constructor", [item(n) for n in ctor_names]))
            assigned.update(ctor_names)
        if method_names:
            cls_items.append(group("methods", [item(n) for n in method_names]))
            assigned.update(method_names)
        if field_names:
            cls_items.append(group("fields", [item(n) for n in field_names]))
            assigned.update(field_names)

        if cls_items:
            structure.append(group(cls.name, cls_items))

    external = sorted([v for v in variables if v.startswith("(")])
    if external:
        structure.append(group("External", [item(v) for v in external]))
        assigned.update(external)

    leftover = sorted([v for v in variables if v not in assigned])
    if leftover:
        structure.append(group("Module-level", [item(v) for v in leftover]))

    return {"@schemaVersion": "1.0", "name": f"{Path(file_name).name} (structure)", "structure": structure}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--neodepends-bin", type=Path, default=Path("./target/release/neodepends"))
    parser.add_argument("--input", type=Path, required=True, help="Focus directory or single Python file (e.g., .../tts/ or .../file.py)")
    parser.add_argument(
        "--project-root",
        type=Path,
        default=None,
        help="Optional project root to run NeoDepends from (default: auto-detect from --input)",
    )
    parser.add_argument(
        "--no-auto-project-root",
        action="store_true",
        help="Disable Python package auto-detection (treat --input as the NeoDepends project root)",
    )
    parser.add_argument("--output-dir", type=Path, required=True, help="Output directory")
    parser.add_argument(
        "--terminal-output",
        type=Path,
        default=None,
        help="Where to save terminal output (default: <output-dir>/terminal_output.txt)",
    )
    parser.add_argument("--no-terminal-output", action="store_true", help="Disable terminal output saving")
    parser.add_argument("--resolver", choices=["depends", "stackgraphs"], default="depends")
    parser.add_argument(
        "--stackgraphs-python-mode",
        choices=["ast", "use-only"],
        default="ast",
        help="StackGraphs mode for Python: ast (default) classifies into Import/Extend/Call/Create when possible; use-only emits only Use edges",
    )
    parser.add_argument(
        "--filter-stackgraphs-false-positives",
        action="store_true",
        default=False,
        help=(
            "Run filter_false_positives.py on the StackGraphs raw DB (dependencies.raw.db) before Python enhancement, "
            "writing dependencies.raw.filtered.db and using it for the final enhanced DV8 exports. "
            "This is meant to remove StackGraphs 'scope bleed' false positives at method/field definition lines."
        ),
    )
    parser.add_argument(
        "--filter-false-positives-script",
        type=Path,
        default=None,
        help="Path to filter_false_positives.py (default: use filter_false_positives.py from 00_NEODEPENDS/)",
    )
    parser.add_argument(
        "--experiment-all",
        action="store_true",
        help="Run: depends + stackgraphs(ast) + stackgraphs(use-only) into subfolders and write a combined comparison file",
    )
    parser.add_argument(
        "--compare-resolvers",
        action="store_true",
        help="Run both resolvers (depends + stackgraphs) into subfolders and write a comparison report",
    )
    parser.add_argument("--langs", default="python", help="Comma-separated language list (default: python)")
    parser.add_argument("--depends-jar", type=Path, default=None)
    parser.add_argument("--depends-java", type=str, default=None)
    parser.add_argument("--depends-xmx", type=str, default=None)
    parser.add_argument(
        "--enhance-script",
        type=Path,
        default=None,
        help="Path to enhance_python_deps.py (default: use enhance_python_deps.py from the NEODEPENDS_DEICIDE workspace)",
    )
    parser.add_argument("--no-enhance", action="store_true", help="Skip Python enhancement step")
    parser.add_argument(
        "--include-external-targets",
        action="store_true",
        default=True,
        help="Include external targets (classes/methods outside the current file) in DV8 export (default: on)",
    )
    parser.add_argument(
        "--exclude-external-targets",
        action="store_true",
        help="Do not include external targets in DV8 export",
    )
    parser.add_argument(
        "--only-py",
        action="store_true",
        default=True,
        help="Only export DV8 for *.py files (default: on)",
    )
    parser.add_argument(
        "--per-file-dv8-clustering",
        action="store_true",
        default=True,
        help="Write `dv8_deps/*.dv8-clustering.json` grouping by class into self/constructor/methods/fields (default: on)",
    )
    parser.add_argument(
        "--no-per-file-dv8-clustering",
        action="store_true",
        help="Disable per-file DV8 clustering export",
    )
    parser.add_argument(
        "--per-file-dbs",
        action="store_true",
        default=True,
        help="Also export one small SQLite DB per file into per_file_dbs/ (default: on)",
    )
    parser.add_argument(
        "--no-per-file-dbs",
        action="store_true",
        help="Disable per-file SQLite DB export",
    )
    parser.add_argument(
        "--include-incoming",
        action="store_true",
        default=True,
        help="Include incoming deps in per-file DBs (src outside file, tgt inside file) (default: on)",
    )
    parser.add_argument(
        "--exclude-incoming",
        action="store_true",
        help="Do not include incoming deps in per-file DBs",
    )
    parser.add_argument(
        "--file-level-dv8",
        action="store_true",
        default=False,
        help="Export an aggregated file-level DV8 dependency matrix (default: off)",
    )
    parser.add_argument(
        "--no-file-level-dv8",
        action="store_true",
        help="Disable aggregated file-level DV8 dependency export",
    )
    parser.add_argument(
        "--file-level-include-external-target-files",
        action="store_true",
        default=True,
        help="Include targets outside the focus prefix as '(External File) ...' (default: on)",
    )
    parser.add_argument(
        "--file-level-exclude-external-target-files",
        action="store_true",
        help="Do not include targets outside the focus prefix in file-level DV8 export",
    )
    parser.add_argument(
        "--file-level-include-self-edges",
        action="store_true",
        default=False,
        help="Include self edges (file -> same file) in file-level DV8 export (default: off)",
    )
    parser.add_argument(
        "--full-dv8",
        action="store_true",
        default=True,
        help="Export a single full DV8 DSM for drill-down (default: on)",
    )
    parser.add_argument(
        "--align-handcount",
        "--filter-architecture",
        action="store_true",
        default=False,
        help="Apply architecture-DSM filtering for DV8 exports: core kinds, strict src/tgt shapes, unique-edge dedupe, deterministic ordering",
    )
    parser.add_argument(
        "--match-to-ts-config",
        action="store_true",
        default=False,
        help="Shortcut for the TrainTicketSystem_TOY_PYTHON_FIRST alignment config: enables --filter-architecture, forces --dv8-hierarchy=handcount, and disables external/incoming edges in exports",
    )
    parser.add_argument(
        "--dv8-hierarchy",
        choices=["handcount", "structured", "flat", "professor"],
        default="structured",
        help="Naming hierarchy for DV8 exports. 'structured' keeps per-class folders (no CLASSES folder). 'flat' removes per-class folders but keeps METHODS/FIELDS/CONSTRUCTORS folders. 'professor' is a deprecated alias for 'structured'.",
    )
    parser.add_argument(
        "--no-full-dv8",
        action="store_true",
        help="Disable full DV8 DSM + clustering export",
    )
    parser.add_argument(
        "--config",
        choices=["automatic", "default", "python", "java", "manual"],
        default="manual",
        help=(
            "Configuration preset. "
            "'automatic': Auto-detect language from files and apply best practices. "
            "'default': Use --langs flag to determine preset (requires explicit --langs). "
            "'python': Python best practices (stackgraphs + ast + structured + filtering). "
            "'java': Java best practices (depends + structured + filtering). "
            "'manual': Specify all options explicitly (default). "
            "Explicit flags override preset values."
        ),
    )

    args = parser.parse_args()

    # Apply config presets
    if args.config in ("automatic", "default", "python", "java"):
        preset_type = args.config

        # Auto-detect language from file extensions for 'automatic' preset
        if preset_type == "automatic":
            # Scan input directory for file extensions
            input_path = Path(args.input)
            if input_path.is_file():
                # Single file - detect from extension
                if input_path.suffix == ".py":
                    preset_type = "python"
                elif input_path.suffix == ".java":
                    preset_type = "java"
                else:
                    preset_type = "python"  # Fallback
            else:
                # Directory - scan for predominant language
                py_count = sum(1 for _ in input_path.rglob("*.py"))
                java_count = sum(1 for _ in input_path.rglob("*.java"))
                if java_count > py_count:
                    preset_type = "java"
                else:
                    preset_type = "python"  # Default to Python

        # Detect language from --langs flag for 'default' preset
        elif preset_type == "default":
            # Check args.langs to detect language (explicit language required)
            langs_list = [x.strip().lower() for x in args.langs.split(",") if x.strip()]
            if "python" in langs_list:
                preset_type = "python"
            elif "java" in langs_list:
                preset_type = "java"
            else:
                # Error: --config default requires explicit --langs
                raise ValueError(
                    "--config default requires explicit --langs flag. "
                    "Use '--langs python' or '--langs java', or use '--config automatic' for auto-detection."
                )

        # Apply Python preset
        if preset_type == "python":
            # Set defaults if not explicitly overridden
            if args.resolver == "depends":  # User didn't change default
                args.resolver = "stackgraphs"
            if args.stackgraphs_python_mode == "use-only":  # User didn't change default
                args.stackgraphs_python_mode = "ast"
            # dv8_hierarchy already defaults to "structured"
            args.align_handcount = True  # Enable --filter-architecture
            args.filter_stackgraphs_false_positives = True

        # Apply Java preset
        elif preset_type == "java":
            # args.resolver already defaults to "depends"
            # dv8_hierarchy already defaults to "structured"
            args.align_handcount = True  # Enable --filter-architecture

    neodepends_bin: Path = _resolve_path_arg(
        args.neodepends_bin, prefer_agent_root=False, must_exist=True, kind="NeoDepends binary"
    )
    focus_path: Path = _resolve_path_arg(args.input, prefer_agent_root=True, must_exist=True, kind="Input path")
    if not focus_path.exists():
        raise FileNotFoundError(f"Input path does not exist: {focus_path}")
    if not (focus_path.is_dir() or focus_path.is_file()):
        raise ValueError(f"Input path must be a file or directory: {focus_path}")

    output_root: Path = _resolve_path_arg(
        args.output_dir, prefer_agent_root=True, must_exist=False, kind="Output directory"
    )
    output_root.mkdir(parents=True, exist_ok=True)

    langs = [x.strip() for x in args.langs.split(",") if x.strip()]
    include_external = bool(args.include_external_targets) and not bool(args.exclude_external_targets)
    per_file = bool(args.per_file_dbs) and not bool(args.no_per_file_dbs)
    include_incoming = bool(args.include_incoming) and not bool(args.exclude_incoming)
    per_file_clustering = bool(args.per_file_dv8_clustering) and not bool(args.no_per_file_dv8_clustering)
    file_level_dv8 = bool(args.file_level_dv8) and not bool(args.no_file_level_dv8)
    full_dv8 = bool(args.full_dv8) and not bool(args.no_full_dv8)
    file_level_include_external = bool(args.file_level_include_external_target_files) and not bool(
        args.file_level_exclude_external_target_files
    )

    enhance_script = args.enhance_script
    if enhance_script is None:
        # Prefer the vendored script inside this repo for self-contained releases.
        local = Path(__file__).resolve().parent / "enhance_python_deps.py"
        enhance_script = local if local.exists() else (Path(__file__).resolve().parents[3] / "enhance_python_deps.py")

    filter_fp_script = args.filter_false_positives_script
    if filter_fp_script is None:
        # Prefer the vendored script inside this repo for self-contained releases.
        local = Path(__file__).resolve().parent / "filter_false_positives.py"
        filter_fp_script = local if local.exists() else (Path(__file__).resolve().parents[2] / "filter_false_positives.py")

    def detect_project_root_and_focus_prefix() -> Tuple[Path, Optional[str]]:
        """
        Choose the NeoDepends project root and a focus prefix for exports.

        For StackGraphs + Python imports, resolution works best if file paths include
        the top-level package folder (e.g. `tts/...`). If the user passes `.../tts`
        as --input and the code imports `tts.*`, we run NeoDepends from the parent
        directory and export only files with prefix `tts/`.

        If --input is a single file, use its parent directory as project root.
        """
        # Handle single file input
        if focus_path.is_file():
            parent_dir = focus_path.parent
            file_name = focus_path.name

            if args.project_root is not None:
                # If explicit project root provided, use it
                project_root = _resolve_path_arg(
                    args.project_root, prefer_agent_root=True, must_exist=True, kind="Project root"
                )
                if not project_root.is_dir():
                    raise NotADirectoryError(f"Project root is not a directory: {project_root}")
                try:
                    rel = focus_path.resolve().relative_to(project_root.resolve())
                    return project_root, str(rel.as_posix()) + "/"
                except Exception:
                    return parent_dir, f"{file_name}/"
            elif args.no_auto_project_root or "python" not in langs:
                # Use parent as project root, file name as focus prefix
                return parent_dir, f"{file_name}/"
            else:
                # Try to detect if parent directory is a Python package
                if (parent_dir / "__init__.py").exists():
                    # Parent is a package, use its parent as project root
                    pkg_name = parent_dir.name
                    return parent_dir.parent, f"{pkg_name}/{file_name}/"
                else:
                    # Parent is not a package, use it as project root
                    return parent_dir, f"{file_name}/"

        # Handle directory input (existing logic)
        if args.project_root is not None:
            project_root = _resolve_path_arg(
                args.project_root, prefer_agent_root=True, must_exist=True, kind="Project root"
            )
            if not project_root.is_dir():
                raise NotADirectoryError(f"Project root is not a directory: {project_root}")
            try:
                rel = focus_path.resolve().relative_to(project_root.resolve())
                rel_s = rel.as_posix()
                focus_prefix = None if rel_s == "." else f"{rel_s.rstrip('/')}/"
            except Exception:
                focus_prefix = None
            return project_root, focus_prefix

        if args.no_auto_project_root or "python" not in langs:
            return focus_path, None

        pkg_init = focus_path / "__init__.py"
        pkg_name = focus_path.name
        if not pkg_init.exists() or not pkg_name:
            return focus_path, None

        # Heuristic: if any file mentions `from <pkg>.` or `import <pkg>`,
        # treat parent as project root so files are named `<pkg>/...`.
        import_pat_1 = f"from {pkg_name}."
        import_pat_2 = f"import {pkg_name}"
        found = False
        try:
            for py in focus_path.rglob("*.py"):
                txt = py.read_text(encoding="utf-8", errors="ignore")
                if import_pat_1 in txt or import_pat_2 in txt:
                    found = True
                    break
        except Exception:
            found = False

        if not found:
            return focus_path, None

        return focus_path.parent, f"{pkg_name}/"

    project_root, focus_prefix = detect_project_root_and_focus_prefix()
    include_root_py = focus_prefix is not None
    dv8_hierarchy = str(args.dv8_hierarchy)
    if dv8_hierarchy == "professor":
        dv8_hierarchy = "structured"
    align_handcount = bool(args.align_handcount) or bool(args.match_to_ts_config)
    if args.match_to_ts_config:
        dv8_hierarchy = "handcount"
        include_external = False
        include_incoming = False
        file_level_include_external = False
        per_file_clustering = False

    def run_one(*, resolver: str, out_dir: Path, stackgraphs_python_mode_override: Optional[str] = None) -> Dict[str, Any]:
        out_dir.mkdir(parents=True, exist_ok=True)
        data_dir = out_dir / "data"
        data_dir.mkdir(parents=True, exist_ok=True)
        terminal_path = args.terminal_output
        if terminal_path is None:
            terminal_path = data_dir / "terminal_output.txt"
        terminal_path = _resolve_path_arg(
            terminal_path, prefer_agent_root=True, must_exist=False, kind="Terminal output"
        )

        logger: Any
        if args.no_terminal_output:
            logger = _StdoutLogger()
        else:
            logger = _Logger(terminal_path)
        try:
            logger.line(f"timestamp: {_dt.datetime.now().isoformat()}")
            logger.line(f"resolver: {resolver}")
            logger.line(f"project_root: {project_root}")
            logger.line(f"focus_path: {focus_path}")
            if focus_prefix:
                logger.line(f"focus_prefix: {focus_prefix}")
            logger.line(f"output: {out_dir}")
            logger.line("")

            stackgraphs_mode = stackgraphs_python_mode_override or args.stackgraphs_python_mode
            option_tag = resolver if resolver == "depends" else f"stackgraphs_{_safe_tag(stackgraphs_mode)}"

            # Main output file (user-facing, simple name)
            full_dep_out_path = out_dir / "analysis-result.json"

            # Database and intermediate files (moved to data/ subdirectory)
            db_path = data_dir / f"dependencies.{option_tag}.db"
            file_level_out_path = data_dir / f"dependencies.{option_tag}.file.dv8-dsm-v3.json"
            raw_db_path = data_dir / f"dependencies.{option_tag}.raw.db"
            filtered_raw_db_path = data_dir / f"dependencies.{option_tag}.raw_filtered.db"
            raw_full_dep_out_path = data_dir / f"dependencies.{option_tag}.raw.dv8-dsm-v3.json"
            raw_filtered_full_dep_out_path = data_dir / f"dependencies.{option_tag}.raw_filtered.dv8-dsm-v3.json"
            raw_file_level_out_path = data_dir / f"dependencies.{option_tag}.raw_file.dv8-dsm-v3.json"
            raw_filtered_file_level_out_path = data_dir / f"dependencies.{option_tag}.raw_filtered_file.dv8-dsm-v3.json"

            # Intermediate directories (move to details/ subdirectory)
            raw_out_dir = data_dir / "raw"
            raw_filtered_out_dir = data_dir / "raw_filtered"

            t_neodep = _run_and_tee([str(neodepends_bin), "--version"], logger=logger)
            _ = t_neodep  # keep elapsed for future if needed

            t1 = time.time()
            run_neodepends(
                neodepends_bin=neodepends_bin,
                input_dir=project_root,
                db_out=db_path,
                resolver=resolver,
                langs=langs,
                depends_jar=args.depends_jar,
                java_bin=args.depends_java,
                xmx=args.depends_xmx,
                stackgraphs_python_mode=stackgraphs_mode,
                logger=logger,
            )
            elapsed_neodepends = time.time() - t1

            elapsed_enhance = 0.0
            raw_exported = False
            raw_filtered_exported = False
            used_filtered_db = False
            if "python" in langs:
                shutil.copyfile(db_path, raw_db_path)

                # Export a raw snapshot before any enhancement mutates the DB.
                t_raw = time.time()
                export_dv8_per_file(
                    db_path=raw_db_path,
                    out_dir=raw_out_dir,
                    include_external_targets=include_external,
                    include_incoming_edges=include_incoming,
                    only_py=args.only_py,
                    focus_prefix=focus_prefix,
                    include_root_py=include_root_py,
                    write_clustering=per_file_clustering,
                    align_handcount=align_handcount,
                    dv8_hierarchy=dv8_hierarchy,
                )
                if file_level_dv8:
                    export_dv8_file_level(
                        db_path=raw_db_path,
                        out_dir=raw_out_dir,
                        output_path=raw_file_level_out_path,
                        focus_prefix=focus_prefix,
                        include_root_py=include_root_py,
                        include_external_target_files=file_level_include_external,
                        include_self_edges=bool(args.file_level_include_self_edges),
                        align_handcount=align_handcount,
                        dv8_hierarchy=dv8_hierarchy,
                    )
                if full_dv8:
                    export_dv8_full_project(
                        db_path=raw_db_path,
                        out_dir=raw_out_dir,
                        output_path=raw_full_dep_out_path,
                        focus_prefix=focus_prefix,
                        include_root_py=include_root_py,
                        include_external_targets=include_external,
                        include_external_target_files=file_level_include_external,
                        include_self_edges=bool(args.file_level_include_self_edges),
                        align_handcount=align_handcount,
                        dv8_hierarchy=dv8_hierarchy,
                    )
                elapsed_raw_export = time.time() - t_raw
                raw_exported = True

                # Optional StackGraphs-only false-positive filtering stage (pre-enhancement).
                #
                # We intentionally filter the *raw* DB before enhancement runs, because the enhancement
                # step inserts deps at method_start rows and could be incorrectly removed by the filter.
                if resolver == "stackgraphs" and bool(args.filter_stackgraphs_false_positives):
                    if not filter_fp_script.exists():
                        raise FileNotFoundError(f"false-positive filter script not found: {filter_fp_script}")
                    if filtered_raw_db_path.exists():
                        filtered_raw_db_path.unlink()

                    run_stackgraphs_false_positive_filter(
                        filter_script=filter_fp_script,
                        input_db=raw_db_path,
                        output_db=filtered_raw_db_path,
                        logger=logger,
                    )

                    # Export filtered-raw DV8 snapshots for debugging.
                    export_dv8_per_file(
                        db_path=filtered_raw_db_path,
                        out_dir=raw_filtered_out_dir,
                        include_external_targets=include_external,
                        include_incoming_edges=include_incoming,
                        only_py=args.only_py,
                        focus_prefix=focus_prefix,
                        include_root_py=include_root_py,
                        write_clustering=per_file_clustering,
                        align_handcount=align_handcount,
                        dv8_hierarchy=dv8_hierarchy,
                    )
                    if file_level_dv8:
                        export_dv8_file_level(
                            db_path=filtered_raw_db_path,
                            out_dir=raw_filtered_out_dir,
                            output_path=raw_filtered_file_level_out_path,
                            focus_prefix=focus_prefix,
                            include_root_py=include_root_py,
                            include_external_target_files=file_level_include_external,
                            include_self_edges=bool(args.file_level_include_self_edges),
                            align_handcount=align_handcount,
                            dv8_hierarchy=dv8_hierarchy,
                        )
                    if full_dv8:
                        export_dv8_full_project(
                            db_path=filtered_raw_db_path,
                            out_dir=raw_filtered_out_dir,
                            output_path=raw_filtered_full_dep_out_path,
                            focus_prefix=focus_prefix,
                            include_root_py=include_root_py,
                            include_external_targets=include_external,
                            include_external_target_files=file_level_include_external,
                            include_self_edges=bool(args.file_level_include_self_edges),
                            align_handcount=align_handcount,
                            dv8_hierarchy=dv8_hierarchy,
                        )
                    raw_filtered_exported = True

                    # Use the filtered raw DB as the base for enhancement + final DV8 exports.
                    shutil.copyfile(filtered_raw_db_path, db_path)
                    used_filtered_db = True

                if not args.no_enhance:
                    if not enhance_script.exists():
                        raise FileNotFoundError(f"enhance script not found: {enhance_script}")
                    t2 = time.time()
                    run_python_enhancement(
                        enhance_script=enhance_script,
                        db_path=db_path,
                        profile=resolver,
                        logger=logger,
                    )
                    elapsed_enhance = time.time() - t2
            else:
                elapsed_raw_export = 0.0

            t3 = time.time()
            export_dv8_per_file(
                db_path=db_path,
                out_dir=data_dir,
                include_external_targets=include_external,
                include_incoming_edges=include_incoming,
                only_py=args.only_py,
                focus_prefix=focus_prefix,
                include_root_py=include_root_py,
                write_clustering=per_file_clustering,
                align_handcount=align_handcount,
                dv8_hierarchy=dv8_hierarchy,
            )
            elapsed_dv8 = time.time() - t3

            if file_level_dv8:
                export_dv8_file_level(
                    db_path=db_path,
                    out_dir=data_dir,
                    output_path=file_level_out_path,
                    focus_prefix=focus_prefix,
                    include_root_py=include_root_py,
                    include_external_target_files=file_level_include_external,
                    include_self_edges=bool(args.file_level_include_self_edges),
                    align_handcount=align_handcount,
                    dv8_hierarchy=dv8_hierarchy,
                )
            if full_dv8:
                export_dv8_full_project(
                    db_path=db_path,
                    out_dir=data_dir,
                    output_path=full_dep_out_path,
                    focus_prefix=focus_prefix,
                    include_root_py=include_root_py,
                    include_external_targets=include_external,
                    include_external_target_files=file_level_include_external,
                    include_self_edges=bool(args.file_level_include_self_edges),
                    align_handcount=align_handcount,
                    dv8_hierarchy=dv8_hierarchy,
                )

            elapsed_per_file = 0.0
            if per_file:
                t4 = time.time()
                export_per_file_dbs(
                    db_path=db_path,
                    out_dir=data_dir,
                    include_incoming_edges=include_incoming,
                    only_py=args.only_py,
                    focus_prefix=focus_prefix,
                    include_root_py=include_root_py,
                )
                elapsed_per_file = time.time() - t4

            summary = {
                "resolver": resolver,
                "option_tag": option_tag,
                "stackgraphs_python_mode": stackgraphs_mode if resolver == "stackgraphs" else None,
                "dv8_hierarchy": dv8_hierarchy,
                "filter_architecture": align_handcount,
                "filter_stackgraphs_false_positives": bool(args.filter_stackgraphs_false_positives),
                "used_filtered_raw_db": used_filtered_db,
                "input_dir": str(focus_path),
                "project_root": str(project_root),
                "focus_prefix": focus_prefix,
                "output_dir": str(out_dir),
                "db_path": str(db_path),
                "raw_db_path": str(raw_db_path) if raw_db_path.exists() else None,
                "filtered_raw_db_path": str(filtered_raw_db_path) if filtered_raw_db_path.exists() else None,
                "raw_output_dir": str(raw_out_dir) if raw_exported else None,
                "raw_dv8_dir": str(raw_out_dir / "dv8_deps") if raw_exported else None,
                "raw_filtered_output_dir": str(raw_filtered_out_dir) if raw_filtered_exported else None,
                "raw_filtered_dv8_dir": str(raw_filtered_out_dir / "dv8_deps") if raw_filtered_exported else None,
                "raw_file_level_dv8_path": str(raw_file_level_out_path) if raw_exported and file_level_dv8 else None,
                "raw_filtered_file_level_dv8_path": str(raw_filtered_file_level_out_path)
                if raw_filtered_exported and file_level_dv8
                else None,
                "raw_full_dv8_dependency_root_path": str(raw_full_dep_out_path)
                if raw_exported and full_dv8
                else None,
                "dv8_dir": str(data_dir / "dv8_deps"),
                "full_dv8_dependency_path": str(full_dep_out_path)
                if full_dv8
                else None,
                "file_level_dv8_path": str(file_level_out_path) if file_level_dv8 else None,
                "raw_filtered_full_dv8_dependency_root_path": str(raw_filtered_full_dep_out_path)
                if raw_filtered_exported and full_dv8
                else None,
                "per_file_dbs_dir": str(data_dir / "per_file_dbs") if per_file else None,
                "data_dir": str(data_dir),
                "timings_sec": {
                    "neodepends": elapsed_neodepends,
                    "raw_dv8_export": elapsed_raw_export,
                    "enhance": elapsed_enhance,
                    "dv8_export": elapsed_dv8,
                    "per_file_db_export": elapsed_per_file,
                },
                "db_summary": _summarize_db(db_path),
                "dv8_summary": _summarize_dv8_dir(data_dir / "dv8_deps"),
                "raw_db_summary": _summarize_db(raw_db_path) if raw_db_path.exists() else None,
                "raw_dv8_summary": _summarize_dv8_dir(raw_out_dir / "dv8_deps") if raw_exported else None,
                "filtered_raw_db_summary": _summarize_db(filtered_raw_db_path) if filtered_raw_db_path.exists() else None,
                "raw_filtered_dv8_summary": _summarize_dv8_dir(raw_filtered_out_dir / "dv8_deps")
                if raw_filtered_exported
                else None,
            }
            (data_dir / "run_summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")

            logger.line("")
            logger.line(f"[OK] Main DB: {db_path}")
            if full_dv8:
                logger.line(f"[OK] Main DV8 DSM: {full_dep_out_path}")
            logger.line("")
            logger.line(f"[OK] Additional files in: {data_dir}")
            if file_level_dv8:
                logger.line(f"  - File-level DV8: {file_level_out_path}")
            if raw_db_path.exists():
                logger.line(f"  - Raw DB (pre-enhancement): {raw_db_path}")
                if raw_exported:
                    logger.line(f"  - Raw DV8 per-file deps: {raw_out_dir / 'dv8_deps'}")
                    if file_level_dv8:
                        logger.line(f"  - Raw DV8 file-level: {raw_file_level_out_path}")
                    if full_dv8:
                        logger.line(f"  - Raw DV8 full: {raw_full_dep_out_path}")
            if filtered_raw_db_path.exists():
                logger.line(f"  - Filtered raw DB (pre-enhancement): {filtered_raw_db_path}")
                if raw_filtered_exported:
                    logger.line(f"  - Filtered raw DV8 per-file: {raw_filtered_out_dir / 'dv8_deps'}")
                    if file_level_dv8:
                        logger.line(f"  - Filtered raw DV8 file-level: {raw_filtered_file_level_out_path}")
                    if full_dv8:
                        logger.line(f"  - Filtered raw DV8 full: {raw_filtered_full_dep_out_path}")
            logger.line(f"  - DV8 per-file deps: {data_dir / 'dv8_deps'}")
            if per_file:
                logger.line(f"  - Per-file DBs: {data_dir / 'per_file_dbs'}")
            logger.line(f"  - Run summary: {data_dir / 'run_summary.json'}")
            if not args.no_terminal_output:
                logger.line(f"[OK] Terminal output: {terminal_path}")

            return summary
        finally:
            logger.close()

    if args.experiment_all:
        depends_dir = output_root / "depends"
        sg_ast_dir = output_root / "stackgraphs_ast"
        sg_useonly_dir = output_root / "stackgraphs_useonly"

        depends_summary = run_one(resolver="depends", out_dir=depends_dir)
        sg_ast_summary = run_one(resolver="stackgraphs", out_dir=sg_ast_dir, stackgraphs_python_mode_override="ast")
        sg_useonly_summary = run_one(
            resolver="stackgraphs", out_dir=sg_useonly_dir, stackgraphs_python_mode_override="use-only"
        )

        comparison: Dict[str, Any] = {
            "input_dir": str(focus_dir),
            "project_root": str(project_root),
            "focus_prefix": focus_prefix,
            "include_root_py": include_root_py,
            "output_root": str(output_root),
            "depends": depends_summary,
            "stackgraphs_ast": sg_ast_summary,
            "stackgraphs_useonly": sg_useonly_summary,
        }
        (output_root / "comparison_all.json").write_text(json.dumps(comparison, indent=2), encoding="utf-8")
        print(f"[OK] Comparison written: {output_root / 'comparison_all.json'}")
        return 0

    if args.compare_resolvers:
        depends_dir = output_root / "depends"
        stack_dir = output_root / "stackgraphs_ast"

        depends_summary = run_one(resolver="depends", out_dir=depends_dir)
        stack_summary = run_one(resolver="stackgraphs", out_dir=stack_dir)

        comparison: Dict[str, Any] = {
            "input_dir": str(focus_dir),
            "project_root": str(project_root),
            "focus_prefix": focus_prefix,
            "output_root": str(output_root),
            "depends": depends_summary,
            "stackgraphs": stack_summary,
            "diff": {},
        }

        def _diff_counts(a: Dict[str, int], b: Dict[str, int]) -> Dict[str, int]:
            keys = set(a.keys()) | set(b.keys())
            return {k: int(a.get(k, 0) - b.get(k, 0)) for k in sorted(keys)}

        comparison["diff"]["db_deps_by_kind_depends_minus_stackgraphs"] = _diff_counts(
            depends_summary["db_summary"]["deps_by_kind"], stack_summary["db_summary"]["deps_by_kind"]
        )
        comparison["diff"]["dv8_totals_depends_minus_stackgraphs"] = _diff_counts(
            depends_summary["dv8_summary"]["totals"], stack_summary["dv8_summary"]["totals"]
        )

        (output_root / "comparison.json").write_text(json.dumps(comparison, indent=2), encoding="utf-8")

        lines = []
        lines.append(f"focus_path: {focus_path}")
        lines.append(f"project_root: {project_root}")
        if focus_prefix:
            lines.append(f"focus_prefix: {focus_prefix}")
        lines.append(f"depends db: {depends_summary.get('db_path')}")
        lines.append(f"stackgraphs db: {stack_summary.get('db_path')}")
        lines.append("")
        lines.append("deps_by_kind (depends - stackgraphs):")
        for k, v in comparison["diff"]["db_deps_by_kind_depends_minus_stackgraphs"].items():
            lines.append(f"  {k}: {v}")
        lines.append("")
        lines.append("dv8_totals (depends - stackgraphs):")
        for k, v in comparison["diff"]["dv8_totals_depends_minus_stackgraphs"].items():
            lines.append(f"  {k}: {v}")
        (output_root / "comparison.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")

        sys.stdout.write(f"[OK] Comparison written: {output_root / 'comparison.txt'}\n")
        sys.stdout.flush()
        return 0

    _ = run_one(resolver=args.resolver, out_dir=output_root)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
