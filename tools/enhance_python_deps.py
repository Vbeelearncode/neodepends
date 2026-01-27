#!/usr/bin/env python3
"""
Python Dependency Enhancement Script

This script post-processes NeoDepends databases to add Method->Field dependencies
for Python code. It scans Python method bodies for self.<field> patterns and creates
USE dependencies from methods to fields.

Usage:
    python3 enhance_python_deps.py <database_path> <source_root>

Example:
    python3 enhance_python_deps.py /tmp/python_god_class_ENHANCED/deps.db /tmp/python_god_class_ENHANCED
"""

import argparse
import sqlite3
import re
import sys
import ast
import textwrap
from pathlib import Path
from typing import Dict, List, Optional, Set, Tuple

def get_file_content(content_id: bytes, conn: sqlite3.Connection) -> str:
    """Fetch file content from the contents table."""
    cursor = conn.cursor()
    cursor.execute("SELECT content FROM contents WHERE id = ?", (content_id,))
    result = cursor.fetchone()
    return result[0] if result else ""

def extract_method_lines(content: str, start_row: int, end_row: int) -> str:
    """Extract method source code by line range."""
    lines = content.split('\n')
    # Row numbers in database are 0-indexed
    if start_row < len(lines) and end_row < len(lines):
        return '\n'.join(lines[start_row:end_row + 1])
    return ""

class _FieldUsageVisitor(ast.NodeVisitor):
    def __init__(self, field_name: str):
        self.field_name = field_name
        self.found = False

    def visit_Attribute(self, node: ast.Attribute):
        # Detect: self.<field_name> (also covers self.<field_name>[...] etc.)
        if (
            isinstance(node.value, ast.Name)
            and node.value.id == "self"
            and node.attr == self.field_name
        ):
            self.found = True
            return
        self.generic_visit(node)

    def visit_Call(self, node: ast.Call):
        # Detect: getattr(self, "<field_name>")
        if (
            isinstance(node.func, ast.Name)
            and node.func.id == "getattr"
            and len(node.args) >= 2
            and isinstance(node.args[0], ast.Name)
            and node.args[0].id == "self"
            and isinstance(node.args[1], ast.Constant)
            and node.args[1].value == self.field_name
        ):
            self.found = True
            return
        self.generic_visit(node)


def _find_field_usages_regex(method_content: str, field_name: str) -> bool:
    """Regex fallback: check if method uses self.<field> pattern."""
    pattern = rf"\bself\.{re.escape(field_name)}\b"

    # Remove Python comments
    lines = method_content.split("\n")
    code_lines = []
    for line in lines:
        if "#" in line:
            line = line.split("#")[0]
        code_lines.append(line)

    code_only = "\n".join(code_lines)
    return bool(re.search(pattern, code_only))


def find_field_usages(method_content: str, field_name: str) -> bool:
    """
    AST-first detection with regex fallback.

    Why: AST avoids false positives (e.g. "self.field" inside strings) and
    detects structured usages like self.field[...] and getattr(self, "field").
    """
    if not method_content.strip():
        return False

    try:
        tree = ast.parse(textwrap.dedent(method_content))
        visitor = _FieldUsageVisitor(field_name)
        visitor.visit(tree)
        return visitor.found
    except SyntaxError:
        # Snippets can fail to parse (indentation, truncation, etc.). Fall back to regex.
        return _find_field_usages_regex(method_content, field_name)


def is_abstract_class(class_node: ast.ClassDef) -> bool:
    """Check if a class is abstract (inherits ABC or uses ABCMeta)."""
    for base in class_node.bases:
        if isinstance(base, ast.Name) and base.id == "ABC":
            return True
        if isinstance(base, ast.Attribute) and base.attr in ("ABC", "ABCMeta"):
            return True
    for keyword in class_node.keywords:
        if keyword.arg == "metaclass":
            if isinstance(keyword.value, ast.Name) and keyword.value.id == "ABCMeta":
                return True
            if isinstance(keyword.value, ast.Attribute) and keyword.value.attr == "ABCMeta":
                return True
    return False


def is_abstract_method(func_node: ast.FunctionDef) -> bool:
    """Check if a method has @abstractmethod decorator."""
    for decorator in func_node.decorator_list:
        if isinstance(decorator, ast.Name) and decorator.id == "abstractmethod":
            return True
        if isinstance(decorator, ast.Attribute) and decorator.attr == "abstractmethod":
            return True
    return False


class _MethodBodyFacts(ast.NodeVisitor):
    """
    Collect AST facts from a function/method snippet.

    Captures a minimal, deterministic subset that helps fill gaps in Python dependency
    extraction (especially for local code):
    - `self.<attr>` occurrences (potential field uses)
    - `self.<method>(...)` calls
    - `super().<method>(...)` calls
    - `var.<method>(...)` calls (recorded; resolved only when `var` type is inferred)
    - `ClassName(...)` constructor calls (Create)
    - local var type bindings from common patterns:
      - `var = ClassName(...)`
      - `var = ClassName.get_instance(...)`
    """

    def __init__(self, known_classes: Set[str]):
        self.known_classes = known_classes
        self.self_attrs: Set[str] = set()
        self.class_attr_uses: Set[Tuple[str, str]] = set()
        self.class_calls: Set[Tuple[str, str]] = set()
        self.self_calls: Set[str] = set()
        self.super_calls: Set[str] = set()
        self.var_calls: List[Tuple[str, str]] = []
        self.creates: Set[str] = set()
        self.cls_create: bool = False
        self.env: Dict[str, str] = {}

    def visit_Attribute(self, node: ast.Attribute):
        if isinstance(node.value, ast.Name) and node.value.id == "self":
            self.self_attrs.add(node.attr)
        if isinstance(node.value, ast.Name) and node.value.id in self.known_classes:
            # Detect ClassName.field usage (class variables / counters)
            self.class_attr_uses.add((node.value.id, node.attr))
        self.generic_visit(node)

    def visit_Assign(self, node: ast.Assign):
        # var = ClassName(...)
        if (
            len(node.targets) == 1
            and isinstance(node.targets[0], ast.Name)
            and isinstance(node.value, ast.Call)
        ):
            var = node.targets[0].id
            # ClassName(...)
            if isinstance(node.value.func, ast.Name) and node.value.func.id in self.known_classes:
                self.env[var] = node.value.func.id
            # ClassName.some_factory(...)
            # Heuristic: classmethod/factory returning an instance of the same class.
            if isinstance(node.value.func, ast.Attribute) and isinstance(node.value.func.value, ast.Name):
                base = node.value.func.value.id
                if base in self.known_classes:
                    self.env[var] = base
            # ClassName.get_instance(...) (explicitly supported singleton pattern)
            if isinstance(node.value.func, ast.Attribute) and isinstance(node.value.func.value, ast.Name):
                base = node.value.func.value.id
                if base in self.known_classes and node.value.func.attr == "get_instance":
                    self.env[var] = base
        self.generic_visit(node)

    def visit_Call(self, node: ast.Call):
        # Create: ClassName(...)
        if isinstance(node.func, ast.Name) and node.func.id in self.known_classes:
            self.creates.add(node.func.id)
            self.generic_visit(node)
            return
        # Create via cls() inside classmethods/singletons
        if isinstance(node.func, ast.Name) and node.func.id == "cls":
            self.cls_create = True
            self.generic_visit(node)
            return

        # Calls: self.m(...), super().m(...), var.m(...)
        if isinstance(node.func, ast.Attribute):
            attr = node.func.attr
            recv = node.func.value

            # Calls: ClassName.method(...), e.g. VideoClip.__init__(self, ...)
            if isinstance(recv, ast.Name) and recv.id in self.known_classes:
                self.class_calls.add((recv.id, attr))
                self.generic_visit(node)
                return

            if isinstance(recv, ast.Name) and recv.id == "self":
                self.self_calls.add(attr)
                self.generic_visit(node)
                return

            if (
                isinstance(recv, ast.Call)
                and isinstance(recv.func, ast.Name)
                and recv.func.id == "super"
            ):
                self.super_calls.add(attr)
                self.generic_visit(node)
                return

            if isinstance(recv, ast.Name):
                self.var_calls.append((recv.id, attr))
                self.generic_visit(node)
                return

        self.generic_visit(node)

def _compress_field_hits(field_names: List[str]) -> str:
    counts: Dict[str, int] = {}
    for name in field_names:
        counts[name] = counts.get(name, 0) + 1
    parts: List[str] = []
    for name in sorted(counts.keys()):
        n = counts[name]
        parts.append(f"{name} x{n}" if n > 1 else name)
    return ", ".join(parts)

def enhance_python_dependencies(db_path: str, source_root: str, *, profile: str = "depends") -> Tuple[int, int]:
    """
    Enhance Python dependencies in a NeoDepends database.

    Returns:
        Tuple of (new_dependencies_added, methods_analyzed)
    """
    conn = sqlite3.Connection(db_path)
    cursor = conn.cursor()
    is_stackgraphs = profile == "stackgraphs"

    # STEP 0: Add File -> File Import deps (internal-only, AST-based).
    cursor.execute("SELECT id, name, content_id FROM entities WHERE kind = 'File'")
    file_rows = cursor.fetchall()
    file_id_by_name: Dict[str, bytes] = {name: fid for fid, name, _cid in file_rows}

    cursor.execute("SELECT src, tgt FROM deps WHERE kind = 'Import'")
    existing_imports: Set[Tuple[bytes, bytes]] = {(s, t) for s, t in cursor.fetchall()}

    def _module_to_file(module: str) -> Optional[str]:
        if not module:
            return None
        as_file = module.replace(".", "/") + ".py"
        if as_file in file_id_by_name:
            return as_file
        as_init = module.replace(".", "/") + "/__init__.py"
        if as_init in file_id_by_name:
            return as_init
        return None

    def _resolve_relative(module: Optional[str], level: int, src_file_name: str) -> Optional[str]:
        if level <= 0:
            return module
        pkg_parts = src_file_name.split("/")[:-1]  # drop filename
        if level > len(pkg_parts):
            return module
        base = pkg_parts[: len(pkg_parts) - (level - 1)]
        if module:
            return ".".join(base + module.split("."))
        return ".".join(base)

    import_added = 0
    step0_changed = False
    for src_file_id, src_file_name, src_content_id in file_rows:
        if not src_file_name.endswith(".py"):
            continue
        content = get_file_content(src_content_id, conn)
        if not content.strip():
            continue
        try:
            tree = ast.parse(content)
        except SyntaxError:
            continue

        targets: Set[str] = set()
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                for alias in node.names:
                    tgt = _module_to_file(alias.name)
                    if tgt:
                        targets.add(tgt)
            elif isinstance(node, ast.ImportFrom):
                abs_mod = _resolve_relative(node.module, getattr(node, "level", 0) or 0, src_file_name)

                # Prefer `from pkg import submodule` as pkg.submodule when it exists.
                if abs_mod:
                    for alias in node.names:
                        tgt = _module_to_file(f"{abs_mod}.{alias.name}")
                        if tgt:
                            targets.add(tgt)

                # Otherwise resolve the base module itself (e.g. `from tts.ticket import Ticket`).
                if abs_mod:
                    tgt = _module_to_file(abs_mod)
                    if tgt:
                        targets.add(tgt)

        for tgt_file_name in sorted(targets):
            tgt_file_id = file_id_by_name.get(tgt_file_name)
            if not tgt_file_id:
                continue
            key = (src_file_id, tgt_file_id)
            if key in existing_imports:
                continue
            row = 0
            cursor.execute(
                "INSERT INTO deps (src, tgt, kind, row, commit_id) VALUES (?, ?, 'Import', ?, NULL)",
                (src_file_id, tgt_file_id, row),
            )
            existing_imports.add(key)
            import_added += 1
            step0_changed = True

    # StackGraphs(+AST) tends to create an additional package-level Import edge:
    #   some_module.py -> pkg/__init__.py
    # when resolving `from pkg.sub import X` or similar references.
    #
    # For our DV8/Deicide workflows (and our TrainTicket handcount ground truth), these edges
    # are treated as noise because the meaningful module dependency is already captured as:
    #   some_module.py -> pkg/sub.py
    #
    # Remove File->File Import edges that target any __init__.py (except self-import).
    if is_stackgraphs:
        cursor.execute(
            """
            DELETE FROM deps
            WHERE kind = 'Import'
              AND src IN (SELECT id FROM entities WHERE kind = 'File')
              AND tgt IN (
                SELECT id FROM entities
                WHERE kind = 'File'
                  AND (name = '__init__.py' OR name LIKE '%/__init__.py')
              )
              AND src != tgt
            """
        )
        removed_init_imports = int(cursor.execute("SELECT changes()").fetchone()[0])
        if removed_init_imports:
            step0_changed = True

        # For StackGraphs, name-binding can create "Use" edges from module-level functions
        # to class fields (e.g., `ticket.ticket_id` inside `main()` -> Ticket.ticket_id).
        #
        # Our handcount/architecture-DSM rules intentionally treat Use as *self.field* access
        # within class methods/constructors only, so remove these module-level Method -> ClassField uses.
        cursor.execute(
            """
            DELETE FROM deps
            WHERE kind = 'Use'
              AND src IN (
                SELECT m.id
                FROM entities m
                JOIN entities f ON f.id = m.parent_id
                WHERE m.kind = 'Method' AND f.kind = 'File'
              )
              AND tgt IN (
                SELECT id
                FROM entities
                WHERE kind = 'Field'
              )
            """
        )
        removed_module_field_uses = int(cursor.execute("SELECT changes()").fetchone()[0])
        if removed_module_field_uses:
            step0_changed = True

    if step0_changed:
        conn.commit()
    print(f"Import deps added: {import_added}")

    new_deps_count = 0
    methods_analyzed = 0

    # Index classes
    #
    # IMPORTANT: Do not key classes only by `name`.
    #
    # - Decorated definitions can be double-tagged depending on tags.scm/tree-sitter,
    #   producing duplicate Class entities with the same name.
    # - Many real projects have multiple classes with the same name in different files.
    #
    # If we collapse name -> single id, we will silently drop class IDs and then fail to:
    # - index their methods/fields
    # - resolve inherited fields
    # which shows up as systematic "missing Use" edges.
    cursor.execute("SELECT id, name, start_row, end_row, content_id FROM entities WHERE kind = 'Class'")
    class_rows = cursor.fetchall()

    class_ids: Set[bytes] = {cid for cid, _name, _sr, _er, _content_id in class_rows}
    class_ids_by_name: Dict[str, List[bytes]] = {}
    class_content_id: Dict[bytes, bytes] = {}
    for cid, name, _sr, _er, content_id in class_rows:
        class_ids_by_name.setdefault(name, []).append(cid)
        class_content_id[cid] = content_id

    known_class_names: Set[str] = set(class_ids_by_name.keys())

    def resolve_class_id_by_name(name: str, preferred_content_id: Optional[bytes]) -> Optional[bytes]:
        """
        Resolve a class name to a specific Class entity ID.

        If multiple classes share the same name, prefer the one in the current file (content_id).
        If still ambiguous, return None (avoid adding false positives).
        """
        ids = class_ids_by_name.get(name) or []
        if not ids:
            return None
        if len(ids) == 1:
            return ids[0]
        # Common case: duplicate tagging of the *same* class (multiple entity IDs with the
        # same name in the same file). In that scenario, picking any one of them is fine and
        # avoids undercounting Create/Extend edges.
        content_ids = {class_content_id.get(cid) for cid in ids}
        if len(content_ids) == 1:
            return ids[0]
        if preferred_content_id is not None:
            for cid in ids:
                if class_content_id.get(cid) == preferred_content_id:
                    return cid
        return None
    # For decorated methods and other tagging edge-cases, infer method->class ownership
    # by span containment within the same file (content_id).
    classes_by_content: Dict[bytes, List[Tuple[int, int, bytes]]] = {}
    for cid, _name, sr, er, content_id in class_rows:
        classes_by_content.setdefault(content_id, []).append((sr, er, cid))
    for content_id in classes_by_content:
        # smallest span first so we pick the most nested/closest containing class
        classes_by_content[content_id].sort(key=lambda t: (t[1] - t[0], t[0], t[1]))

    def infer_owner_class_from_span(content_id: bytes, start_row: int, end_row: int) -> Optional[bytes]:
        spans = classes_by_content.get(content_id) or []
        for sr, er, cid in spans:
            if sr <= start_row and er >= end_row:
                return cid
        return None

    extend_added = 0
    if is_stackgraphs:
        # STEP 0.5: Add missing Extend edges from AST (useful for StackGraphs resolver).
        cursor.execute("SELECT src, tgt FROM deps WHERE kind = 'Extend'")
        existing_extends: Set[Tuple[bytes, bytes]] = {(s, t) for s, t in cursor.fetchall()}

        cursor.execute("SELECT id, name, content_id FROM entities WHERE kind = 'File'")
        file_rows = cursor.fetchall()
        for file_id, file_name, content_id in file_rows:
            if not file_name.endswith(".py"):
                continue
            content = get_file_content(content_id, conn)
            if not content.strip():
                continue
            try:
                tree = ast.parse(content)
            except SyntaxError:
                continue
            for node in ast.walk(tree):
                if not isinstance(node, ast.ClassDef):
                    continue
                src_cid = resolve_class_id_by_name(node.name, content_id)
                if src_cid is None:
                    continue
                for b in node.bases:
                    base_name: Optional[str] = None
                    if isinstance(b, ast.Name):
                        base_name = b.id
                    elif isinstance(b, ast.Attribute):
                        # module.Base -> Base (best effort)
                        base_name = b.attr
                    if base_name is None or base_name not in known_class_names:
                        continue
                    tgt_cid = resolve_class_id_by_name(base_name, content_id)
                    if tgt_cid is None:
                        continue
                    key = (src_cid, tgt_cid)
                    if key in existing_extends:
                        continue
                    cursor.execute(
                        "INSERT INTO deps (src, tgt, kind, row, commit_id) VALUES (?, ?, 'Extend', 0, NULL)",
                        (src_cid, tgt_cid),
                    )
                    existing_extends.add(key)
                    extend_added += 1

        if extend_added:
            conn.commit()

    # Index direct base classes using Extend deps (if present)
    cursor.execute(
        """
        SELECT d.src, d.tgt
        FROM deps d
        JOIN entities e_src ON e_src.id = d.src
        JOIN entities e_tgt ON e_tgt.id = d.tgt
        WHERE d.kind = 'Extend' AND e_src.kind = 'Class' AND e_tgt.kind = 'Class'
        ORDER BY d.row ASC
        """
    )
    bases_by_class: Dict[bytes, List[bytes]] = {}
    for src, tgt in cursor.fetchall():
        bases_by_class.setdefault(src, []).append(tgt)

    # Index methods: class -> {name: id}
    cursor.execute("SELECT id, parent_id, name, start_row, end_row, content_id FROM entities WHERE kind = 'Method'")
    method_rows = cursor.fetchall()
    method_owner_class: Dict[bytes, bytes] = {}
    methods_by_class: Dict[bytes, Dict[str, bytes]] = {}
    for mid, parent_id, name, sr, er, cid in method_rows:
        owner: Optional[bytes] = None
        if parent_id in class_ids:
            owner = parent_id
        else:
            inferred = infer_owner_class_from_span(cid, sr, er)
            if inferred is not None:
                owner = inferred
        if owner is not None:
            method_owner_class[mid] = owner
            methods_by_class.setdefault(owner, {})[name] = mid

    # Index fields: class -> {name: id}. Include fields parented by the class and fields parented by a method under a class.
    cursor.execute("SELECT id, parent_id, name FROM entities WHERE kind = 'Field'")
    field_rows = cursor.fetchall()
    fields_by_class: Dict[bytes, Dict[str, bytes]] = {}
    for fid, parent_id, name in field_rows:
        owner: Optional[bytes] = None
        if parent_id in class_ids:
            owner = parent_id
        elif parent_id in method_owner_class:
            owner = method_owner_class[parent_id]
        if owner is not None:
            fields_by_class.setdefault(owner, {}).setdefault(name, fid)

    def resolve_inherited_field(class_id: bytes, field_name: str) -> Optional[bytes]:
        if field_name in fields_by_class.get(class_id, {}):
            return fields_by_class[class_id][field_name]
        seen: Set[bytes] = set()
        queue: List[bytes] = list(bases_by_class.get(class_id, []))
        while queue:
            b = queue.pop(0)
            if b in seen:
                continue
            seen.add(b)
            if field_name in fields_by_class.get(b, {}):
                return fields_by_class[b][field_name]
            queue.extend(bases_by_class.get(b, []))
        return None

    def resolve_method_in_hierarchy(class_id: bytes, method_name: str) -> Optional[bytes]:
        if method_name in methods_by_class.get(class_id, {}):
            return methods_by_class[class_id][method_name]
        seen: Set[bytes] = set()
        queue: List[bytes] = list(bases_by_class.get(class_id, []))
        while queue:
            b = queue.pop(0)
            if b in seen:
                continue
            seen.add(b)
            if method_name in methods_by_class.get(b, {}):
                return methods_by_class[b][method_name]
            queue.extend(bases_by_class.get(b, []))
        return None

    # Heuristics for Python: var-name -> ClassName (snake_case to CamelCase),
    # and "unique method owner" when a method name appears on exactly one internal class.
    def guess_class_from_var(var: str, preferred_content_id: Optional[bytes]) -> Optional[bytes]:
        camel = "".join(p.capitalize() for p in var.split("_") if p)
        return resolve_class_id_by_name(camel, preferred_content_id)

    unique_method_owner: Dict[str, bytes] = {}
    method_owner_counts: Dict[str, int] = {}
    for cid, methods in methods_by_class.items():
        for mname in methods.keys():
            method_owner_counts[mname] = method_owner_counts.get(mname, 0) + 1
    for cid, methods in methods_by_class.items():
        for mname in methods.keys():
            if method_owner_counts.get(mname) == 1:
                unique_method_owner[mname] = cid

    # Cache existing deps to avoid repeated SQL lookups.
    cursor.execute("SELECT src, tgt, kind FROM deps WHERE kind IN ('Use','Call','Create')")
    existing: Set[Tuple[bytes, bytes, str]] = {(s, t, k) for s, t, k in cursor.fetchall()}

    print(f"Found {len(class_rows)} classes to analyze...")
    print(f"Found {len(method_rows)} methods/functions to analyze...")
    if extend_added:
        print(f"Extend deps added (AST): {extend_added}")

    existing_create_by_src: Dict[bytes, Set[bytes]] = {}
    if is_stackgraphs:
        # Create edges can be noisy from StackGraphs; prune them to actual constructor calls.
        cursor.execute(
            """
            SELECT d.src, d.tgt
            FROM deps d
            JOIN entities e_tgt ON e_tgt.id = d.tgt
            WHERE d.kind = 'Create' AND e_tgt.kind = 'Class'
            """
        )
        for src_id, tgt_id in cursor.fetchall():
            existing_create_by_src.setdefault(src_id, set()).add(tgt_id)

    # Analyze each method/function independently using AST.
    for method_id, parent_id, method_name, method_start, method_end, content_id in method_rows:
        methods_analyzed += 1
        file_content = get_file_content(content_id, conn)
        method_content = extract_method_lines(file_content, method_start, method_end)
        if not method_content.strip():
            continue

        owner_cls_id: Optional[bytes] = None
        if parent_id in class_ids:
            owner_cls_id = parent_id
        else:
            owner_cls_id = method_owner_class.get(method_id) or infer_owner_class_from_span(content_id, method_start, method_end)

        try:
            tree = ast.parse(textwrap.dedent(method_content))
        except SyntaxError:
            # Regex fallback: only within the owning class (no inherited resolution).
            if owner_cls_id is None:
                continue
            local = fields_by_class.get(owner_cls_id, {})
            if not local:
                continue
            hits: List[str] = []
            for fname, fid in local.items():
                if find_field_usages(method_content, fname):
                    key = (method_id, fid, "Use")
                    if key not in existing:
                        cursor.execute(
                            "INSERT INTO deps (src, tgt, kind, row, commit_id) VALUES (?, ?, 'Use', ?, NULL)",
                            (method_id, fid, method_start),
                        )
                        existing.add(key)
                        new_deps_count += 1
                    hits.append(fname)
            if hits:
                print(f"  {method_name} -> {_compress_field_hits(hits)} (Use)")
            continue

        facts = _MethodBodyFacts(known_classes=known_class_names)
        facts.visit(tree)

        # Minimal annotation env from signature: def f(x: Passenger, ...)
        anno_env: Dict[str, str] = {}
        fdef = next((n for n in tree.body if isinstance(n, ast.FunctionDef)), None)
        if fdef is not None:
            for a in fdef.args.args:
                if a.arg in {"self", "cls"}:
                    continue
                if isinstance(a.annotation, ast.Name) and a.annotation.id in known_class_names:
                    anno_env[a.arg] = a.annotation.id

        env: Dict[str, str] = dict(anno_env)
        env.update(facts.env)

        called_names: Set[str] = (
            set(facts.self_calls)
            | set(facts.super_calls)
            | {m for _v, m in facts.var_calls}
            | {m for _cls, m in facts.class_calls}
        )
        resolved_call_targets_by_name: Dict[str, Set[bytes]] = {}

        # (A0) Method -> Field Use edges for ClassName.field (class vars), e.g. Ticket.ticket_counter
        for cls_name, field_name in sorted(facts.class_attr_uses):
            cls_id = resolve_class_id_by_name(cls_name, content_id)
            if cls_id is None:
                continue
            fid = fields_by_class.get(cls_id, {}).get(field_name)
            if fid is None:
                continue
            key = (method_id, fid, "Use")
            if key not in existing:
                cursor.execute(
                    "INSERT INTO deps (src, tgt, kind, row, commit_id) VALUES (?, ?, 'Use', ?, NULL)",
                    (method_id, fid, method_start),
                )
                existing.add(key)
                new_deps_count += 1

        # (A) Method -> Field Use edges for self.<field>, including inherited fields.
        if owner_cls_id is not None:
            used_fields: List[str] = []
            for fname in sorted(facts.self_attrs):
                fid = resolve_inherited_field(owner_cls_id, fname)
                if fid is None:
                    continue
                key = (method_id, fid, "Use")
                if key in existing:
                    continue
                cursor.execute(
                    "INSERT INTO deps (src, tgt, kind, row, commit_id) VALUES (?, ?, 'Use', ?, NULL)",
                    (method_id, fid, method_start),
                )
                existing.add(key)
                new_deps_count += 1
                used_fields.append(fname)
            if used_fields:
                print(f"  {method_name} -> {_compress_field_hits(used_fields)} (Use)")

        # (B) self.method(...) -> Call to resolved method entity.
        if owner_cls_id is not None:
            for callee in sorted(facts.self_calls):
                tgt_mid = resolve_method_in_hierarchy(owner_cls_id, callee)
                if tgt_mid is None:
                    continue
                resolved_call_targets_by_name.setdefault(callee, set()).add(tgt_mid)
                key = (method_id, tgt_mid, "Call")
                if key in existing:
                    continue
                cursor.execute(
                    "INSERT INTO deps (src, tgt, kind, row, commit_id) VALUES (?, ?, 'Call', ?, NULL)",
                    (method_id, tgt_mid, method_start),
                )
                existing.add(key)
                new_deps_count += 1

        # (C) super().method(...) -> Call to direct base method.
        if owner_cls_id is not None and facts.super_calls:
            direct_bases = bases_by_class.get(owner_cls_id, [])
            for callee in sorted(facts.super_calls):
                tgt_mid: Optional[bytes] = None
                for b in direct_bases:
                    tgt_mid = resolve_method_in_hierarchy(b, callee)
                    if tgt_mid is not None:
                        break
                if tgt_mid is None:
                    continue
                resolved_call_targets_by_name.setdefault(callee, set()).add(tgt_mid)
                key = (method_id, tgt_mid, "Call")
                if key in existing:
                    continue
                cursor.execute(
                    "INSERT INTO deps (src, tgt, kind, row, commit_id) VALUES (?, ?, 'Call', ?, NULL)",
                    (method_id, tgt_mid, method_start),
                )
                existing.add(key)
                new_deps_count += 1

        # (C1) ClassName.method(...) -> Call to that class's method (commonly used for base ctors).
        # Example: VideoClip.__init__(self, ...) inside a subclass constructor.
        for cls_name, callee in sorted(facts.class_calls):
            cls_id = resolve_class_id_by_name(cls_name, content_id)
            if cls_id is None:
                continue
            tgt_mid = resolve_method_in_hierarchy(cls_id, callee)
            if tgt_mid is None:
                continue
            resolved_call_targets_by_name.setdefault(callee, set()).add(tgt_mid)
            key = (method_id, tgt_mid, "Call")
            if key in existing:
                continue
            cursor.execute(
                "INSERT INTO deps (src, tgt, kind, row, commit_id) VALUES (?, ?, 'Call', ?, NULL)",
                (method_id, tgt_mid, method_start),
            )
            existing.add(key)
            new_deps_count += 1

        # (D) var.method(...) with inferred var type -> Call.
        for var, callee in facts.var_calls:
            cls_id: Optional[bytes] = None
            cls_name = env.get(var)
            if cls_name:
                cls_id = resolve_class_id_by_name(cls_name, content_id)
            if cls_id is None:
                cls_id = guess_class_from_var(var, content_id)
            if cls_id is None:
                cls_id = unique_method_owner.get(callee)
            if cls_id is None:
                continue

            tgt_mid = resolve_method_in_hierarchy(cls_id, callee)
            if tgt_mid is None:
                continue
            resolved_call_targets_by_name.setdefault(callee, set()).add(tgt_mid)
            key = (method_id, tgt_mid, "Call")
            if key in existing:
                continue
            cursor.execute(
                "INSERT INTO deps (src, tgt, kind, row, commit_id) VALUES (?, ?, 'Call', ?, NULL)",
                (method_id, tgt_mid, method_start),
            )
            existing.add(key)
            new_deps_count += 1

        if is_stackgraphs:
            # (D2) Prune noisy Call deps that don't correspond to any call site in the snippet.
            # Keep call edges only if:
            # - the target method name appears in called_names, AND
            # - if we resolved at least one target for that name, keep only those targets.
            cursor.execute(
                """
                SELECT d.tgt, e_tgt.name
                FROM deps d
                JOIN entities e_tgt ON e_tgt.id = d.tgt
                WHERE d.kind = 'Call' AND d.src = ? AND e_tgt.kind = 'Method'
                """,
                (method_id,),
            )
            for tgt_mid, tgt_name in cursor.fetchall():
                if tgt_name not in called_names:
                    cursor.execute("DELETE FROM deps WHERE kind = 'Call' AND src = ? AND tgt = ?", (method_id, tgt_mid))
                    existing.discard((method_id, tgt_mid, "Call"))
                    continue
                resolved = resolved_call_targets_by_name.get(tgt_name)
                if resolved and tgt_mid not in resolved:
                    cursor.execute("DELETE FROM deps WHERE kind = 'Call' AND src = ? AND tgt = ?", (method_id, tgt_mid))
                    existing.discard((method_id, tgt_mid, "Call"))

        # (E) ClassName(...) -> Create
        allowed_create_targets: Set[bytes] = set()
        for cname in sorted(facts.creates):
            cls_id = resolve_class_id_by_name(cname, content_id)
            if cls_id is None:
                continue
            allowed_create_targets.add(cls_id)
            key = (method_id, cls_id, "Create")
            if key in existing:
                continue
            cursor.execute(
                "INSERT INTO deps (src, tgt, kind, row, commit_id) VALUES (?, ?, 'Create', ?, NULL)",
                (method_id, cls_id, method_start),
            )
            existing.add(key)
            new_deps_count += 1

        if facts.cls_create and owner_cls_id is not None:
            # Treat cls() as creating the owning class.
            allowed_create_targets.add(owner_cls_id)

        if is_stackgraphs:
            # Prune noisy Create deps that are not supported by the method snippet.
            # Keep only allowed_create_targets for internal classes.
            for tgt_id in sorted(existing_create_by_src.get(method_id, set())):
                if tgt_id in allowed_create_targets:
                    continue
                cursor.execute("DELETE FROM deps WHERE kind = 'Create' AND src = ? AND tgt = ?", (method_id, tgt_id))
            existing_create_by_src[method_id] = set(allowed_create_targets)

    conn.commit()

    # STEP 4: Add abstract method Override dependencies
    print(f"\n{'='*70}")
    print("STEP 4: Detecting abstract method overrides...")
    print("="*70)

    override_deps_count = 0

    # Index abstract classes and their abstract methods
    abstract_class_ids: Set[bytes] = set()
    abstract_methods_by_class: Dict[bytes, Dict[str, bytes]] = {}

    for class_id, class_name, class_start, class_end, content_id in class_rows:
        content = get_file_content(content_id, conn)
        if not content.strip():
            continue

        try:
            tree = ast.parse(content)
        except SyntaxError:
            continue

        # Find the matching class node in the AST
        for node in ast.walk(tree):
            if isinstance(node, ast.ClassDef) and node.name == class_name:
                if is_abstract_class(node):
                    abstract_class_ids.add(class_id)

                    # Collect abstract methods
                    for item in node.body:
                        if isinstance(item, ast.FunctionDef) and is_abstract_method(item):
                            method_id = methods_by_class.get(class_id, {}).get(item.name)
                            if method_id:
                                abstract_methods_by_class.setdefault(class_id, {})[item.name] = method_id
                break

    print(f"Found {len(abstract_class_ids)} abstract classes")
    print(f"Found {sum(len(m) for m in abstract_methods_by_class.values())} abstract methods")

    # Build a reverse inheritance map: parent -> set of ALL descendants (transitive)
    # This handles chains like StationManager -> Staff -> Person(ABC)
    # where StationManager overrides Person.display_info through Staff.
    children_by_class: Dict[bytes, Set[bytes]] = {}
    for child_id, parent_ids in bases_by_class.items():
        for parent_id in parent_ids:
            children_by_class.setdefault(parent_id, set()).add(child_id)

    def get_all_descendants(class_id: bytes) -> Set[bytes]:
        """BFS to find all transitive descendants of a class."""
        descendants: Set[bytes] = set()
        queue = list(children_by_class.get(class_id, set()))
        while queue:
            cid = queue.pop(0)
            if cid in descendants:
                continue
            descendants.add(cid)
            queue.extend(children_by_class.get(cid, set()))
        return descendants

    # For each abstract class, find ALL descendant classes that override its abstract methods
    for abstract_class_id, abstract_methods in abstract_methods_by_class.items():
        all_descendants = get_all_descendants(abstract_class_id)

        # Filter to concrete (non-abstract) descendants only
        concrete_descendants = [d for d in all_descendants if d not in abstract_class_ids]

        for child_class_id in concrete_descendants:
            child_methods = methods_by_class.get(child_class_id, {})

            for abstract_method_name, abstract_method_id in abstract_methods.items():
                if abstract_method_name in child_methods:
                    impl_method_id = child_methods[abstract_method_name]

                    # Insert Override dependency: child_method -> parent_abstract_method
                    key = (impl_method_id, abstract_method_id, "Override")
                    if key not in existing:
                        cursor.execute(
                            "INSERT INTO deps (src, tgt, kind, row, commit_id) VALUES (?, ?, 'Override', 0, NULL)",
                            (impl_method_id, abstract_method_id),
                        )
                        existing.add(key)
                        override_deps_count += 1

                        # Log the override
                        cursor.execute(
                            "SELECT e_child.name, e_parent.name FROM entities e_child, entities e_parent WHERE e_child.id = ? AND e_parent.id = ?",
                            (child_class_id, abstract_class_id),
                        )
                        result = cursor.fetchone()
                        if result:
                            child_name, parent_name = result
                            print(f"  {child_name}.{abstract_method_name} -> {parent_name}.{abstract_method_name} (Override)")

    print(f"\n[OK] Added {override_deps_count} Override dependencies")

    conn.commit()
    conn.close()

    return new_deps_count, methods_analyzed, override_deps_count

def fix_field_parent_ids(db_path: str) -> int:
    """
    Fix Field entity parent_ids to be children of their Class instead of a Method.

    This is CRITICAL for Deicide clustering because it only processes dependencies
    between sibling entities (entities sharing the same parent_id).

    NeoDepends may attach fields to the method where the assignment occurs
    (e.g., in setters), which breaks sibling-only clustering and can duplicate
    fields (one under the Class, one under a Method).

    Returns:
        Number of fields updated
    """
    conn = sqlite3.Connection(db_path)
    cursor = conn.cursor()

    print("\n" + "="*70)
    print("FIXING FIELD PARENT IDs")
    print("="*70)

    # Find fields parented by methods whose parent is a class:
    # Field -> Method -> Class. Move Field under Class and merge duplicates by name.
    cursor.execute("""
        SELECT
            f.id, f.name,
            m.id, m.name,
            c.id, c.name
        FROM entities f
        JOIN entities m ON m.id = f.parent_id
        JOIN entities c ON c.id = m.parent_id
        WHERE f.kind = 'Field'
          AND m.kind = 'Method'
          AND c.kind = 'Class'
    """)
    rows = cursor.fetchall()

    if not rows:
        print("[OK] No fields need fixing - all fields already have Class as parent_id")
        conn.close()
        return 0

    print(f"Found {len(rows)} fields parented by Methods")
    print("Updating to have Class as parent (and merging duplicates)...")

    moved_count = 0
    merged_count = 0
    moved_examples: List[str] = []
    merged_examples: List[str] = []
    merged_deps_repointed = 0

    for field_id, field_name, method_id, method_name, class_id, class_name in rows:
        cursor.execute(
            """
            SELECT id
            FROM entities
            WHERE kind = 'Field' AND parent_id = ? AND name = ?
            LIMIT 1
            """,
            (class_id, field_name),
        )
        canonical = cursor.fetchone()

        if canonical and canonical[0] != field_id:
            canonical_id = canonical[0]
            cursor.execute("UPDATE deps SET tgt = ? WHERE tgt = ?", (canonical_id, field_id))
            merged_deps_repointed += cursor.rowcount
            cursor.execute("DELETE FROM deps WHERE src = ? OR tgt = ?", (field_id, field_id))
            cursor.execute("DELETE FROM entities WHERE id = ?", (field_id,))
            merged_count += 1
            if len(merged_examples) < 25:
                merged_examples.append(
                    f"merge: Field '{class_name}.{field_name}' from Method '{class_name}.{method_name}' -> canonical Field '{class_name}.{field_name}'"
                )
        else:
            cursor.execute("UPDATE entities SET parent_id = ? WHERE id = ?", (class_id, field_id))
            moved_count += 1
            if len(moved_examples) < 25:
                moved_examples.append(
                    f"move: Field '{class_name}.{field_name}' from Method '{class_name}.{method_name}' -> Class '{class_name}'"
                )

    conn.commit()

    # After re-parenting/merging, some Use deps can become duplicates
    # (same src/tgt/kind) because multiple field entities collapse into one.
    cursor.execute("""
        DELETE FROM deps
        WHERE kind = 'Use'
          AND rowid NOT IN (
              SELECT MIN(rowid)
              FROM deps
              WHERE kind = 'Use'
              GROUP BY src, tgt, kind
          )
    """)
    deduped_use = cursor.rowcount
    if deduped_use:
        print(f"[INFO] Deduped {deduped_use} duplicate Use deps after field merge")
        conn.commit()

    updated_count = moved_count + merged_count

    # Verify the fix
    cursor.execute("""
        SELECT COUNT(*)
        FROM deps d
        JOIN entities e_src ON d.src = e_src.id
        JOIN entities e_tgt ON d.tgt = e_tgt.id
        WHERE e_src.kind = 'Method'
          AND e_tgt.kind = 'Field'
          AND hex(e_src.parent_id) = hex(e_tgt.parent_id)
    """)

    siblings_count = cursor.fetchone()[0]

    print(f"[OK] Updated {updated_count} field entities ({moved_count} moved, {merged_count} merged)")
    print(f"[OK] {siblings_count} Method->Field dependencies now between siblings")
    print("   (Deicide will now process these dependencies!)")

    if moved_examples:
        print("\n[INFO] Examples (moved fields):")
        for ex in moved_examples:
            print(f"  - {ex}")

    if merged_examples:
        print("\n[INFO] Examples (merged duplicates):")
        for ex in merged_examples:
            print(f"  - {ex}")

    report = {
        "db_path": db_path,
        "updated_fields_total": updated_count,
        "moved_fields": moved_count,
        "merged_fields": merged_count,
        "merged_deps_repointed": merged_deps_repointed,
        "deduped_use_deps": deduped_use,
        "method_field_sibling_use_deps": siblings_count,
        "examples": {
            "moved": moved_examples,
            "merged": merged_examples,
        },
    }
    # Put report in details/ subdirectory if it exists, otherwise same directory as DB
    db_parent = Path(db_path).parent
    details_dir = db_parent / "details"
    if details_dir.exists() and details_dir.is_dir():
        report_path = details_dir / "enhance_python_deps.report.json"
    else:
        report_path = db_parent / "enhance_python_deps.report.json"
    try:
        report_path.write_text(__import__("json").dumps(report, indent=2), encoding="utf-8")
        print(f"[INFO] Wrote report: {report_path}")
    except Exception as exc:
        print(f"[WARN] Failed to write report file: {exc}")

    conn.close()
    return updated_count

def verify_enhancement(db_path: str):
    """Verify that Method->Field dependencies were added."""
    conn = sqlite3.Connection(db_path)
    cursor = conn.cursor()

    # Count Method->Field dependencies
    cursor.execute("""
        SELECT COUNT(*)
        FROM deps d
        JOIN entities e_src ON d.src = e_src.id
        JOIN entities e_tgt ON d.tgt = e_tgt.id
        WHERE e_src.kind = 'Method' AND e_tgt.kind = 'Field' AND d.kind = 'Use'
    """)

    method_field_count = cursor.fetchone()[0]

    # Show dependency breakdown
    cursor.execute("""
        SELECT e_src.kind as src_kind, e_tgt.kind as tgt_kind, d.kind as dep_kind, COUNT(*) as count
        FROM deps d
        JOIN entities e_src ON d.src = e_src.id
        JOIN entities e_tgt ON d.tgt = e_tgt.id
        GROUP BY e_src.kind, e_tgt.kind, d.kind
        ORDER BY count DESC
    """)

    print("\n" + "="*70)
    print("VERIFICATION RESULTS")
    print("="*70)
    print("\nDependency Breakdown:")
    print(f"{'Source':<15} {'Target':<15} {'Type':<10} {'Count':<10}")
    print("-" * 70)

    for row in cursor.fetchall():
        src_kind, tgt_kind, dep_kind, count = row
        print(f"{src_kind:<15} {tgt_kind:<15} {dep_kind:<10} {count:<10}")

    conn.close()

    return method_field_count

def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("database_path", type=str)
    parser.add_argument("source_root", nargs="?", default=None)
    parser.add_argument("--profile", choices=["depends", "stackgraphs"], default="depends")
    args = parser.parse_args()

    db_path = args.database_path
    source_root = args.source_root if args.source_root is not None else str(Path(db_path).parent)
    profile = args.profile

    if not Path(db_path).exists():
        print(f"Error: Database not found at {db_path}")
        return 2

    print("="*70)
    print("Python Dependency Enhancement Tool")
    print("="*70)
    print(f"Database: {db_path}")
    print(f"Source root: {source_root}")
    print()

    # Steps 1-4: Add Method->Field dependencies, fix parents, detect overrides
    print("STEP 1: Adding Method->Field dependencies...")
    print("="*70)
    new_deps, methods, override_count = enhance_python_dependencies(db_path, source_root, profile=profile)

    print(f"\n{'='*70}")
    print(f"Steps 1 & 4 complete!")
    print(f"  Methods analyzed: {methods}")
    print(f"  New dependencies added: {new_deps}")
    print(f"  Override dependencies added: {override_count}")

    # Step 2: Fix Field parent_ids (CRITICAL for Deicide!)
    print("\n" + "="*70)
    print("STEP 2: Fixing Field parent_ids for clustering...")
    print("="*70)
    fields_fixed = fix_field_parent_ids(db_path)

    # Step 3: Verify
    method_field_count = verify_enhancement(db_path)

    print(f"\n{'='*70}")
    print("COMPLETE SUCCESS!")
    print(f"  - {method_field_count} Method->Field dependencies created")
    print(f"  - {override_count} Override dependencies created")
    print(f"  - {fields_fixed} fields now siblings with methods")
    print(f"  - Database ready for Deicide hierarchical clustering!")
    print(f"{'='*70}\n")

if __name__ == "__main__":
    raise SystemExit(main())
