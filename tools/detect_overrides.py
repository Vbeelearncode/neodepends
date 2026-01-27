#!/usr/bin/env python3
"""
Unified Override Detection Script (Python + Java)

This script post-processes NeoDepends databases to add Override dependencies
for both Python and Java code. It detects when child classes override methods
from parent classes, creating maintenance dependencies.

Supported languages:
- Python: Detects @abstractmethod overrides (direct + transitive)
- Java: Detects @Override annotation usage (direct + transitive)

Usage:
    python3 detect_overrides.py <database_path> <source_root>

Example:
    python3 detect_overrides.py /tmp/project/deps.db /tmp/project

Dependencies are added as:
    Extend: child_class -> parent_class (inheritance, if missing)
    Override: child_method -> parent_method (method override)
"""

import argparse
import sqlite3
import sys
import ast
import re
from pathlib import Path
from typing import Dict, List, Set, Tuple, Optional
from collections import defaultdict


# =============================================================================
# Common Database Functions
# =============================================================================

def get_file_content(content_id: bytes, conn: sqlite3.Connection) -> str:
    """Fetch file content from the contents table."""
    cursor = conn.cursor()
    cursor.execute("SELECT content FROM contents WHERE id = ?", (content_id,))
    result = cursor.fetchone()
    return result[0] if result else ""


def get_all_classes(conn: sqlite3.Connection) -> Dict[bytes, Tuple[str, bytes]]:
    """
    Get all classes from the database.

    Returns:
        Dict mapping class_id -> (class_name, content_id)
    """
    cursor = conn.cursor()
    cursor.execute("""
        SELECT e.id, e.name, e.content_id
        FROM entities e
        WHERE e.kind = 'Class'
    """)

    result = {}
    for class_id, class_name, content_id in cursor.fetchall():
        result[class_id] = (class_name, content_id)
    return result


def get_class_by_name(conn: sqlite3.Connection, class_name: str, content_id: bytes) -> Optional[bytes]:
    """Find class ID by name and content_id (file)."""
    cursor = conn.cursor()
    cursor.execute("""
        SELECT id
        FROM entities
        WHERE kind = 'Class' AND name = ? AND content_id = ?
    """, (class_name, content_id))

    result = cursor.fetchone()
    return result[0] if result else None


def get_class_methods(conn: sqlite3.Connection, class_id: bytes) -> Dict[str, bytes]:
    """
    Get all methods belonging to a class.

    Returns:
        Dict mapping method_name -> method_id
    """
    cursor = conn.cursor()
    cursor.execute("""
        SELECT id, name
        FROM entities
        WHERE parent_id = ? AND kind = 'Method'
    """, (class_id,))

    return {name: method_id for method_id, name in cursor.fetchall()}


def build_all_ancestors(class_id: bytes, inheritance: Dict[bytes, List[bytes]],
                        visited: Set[bytes] = None) -> List[bytes]:
    """
    Recursively build list of ALL ancestors (parents, grandparents, etc.).

    This enables detection of transitive overrides where a class overrides
    a method from a grandparent through an intermediate parent.

    Example:
        Person.display_info() is abstract
        Staff extends Person but doesn't implement display_info
        TicketAgent extends Staff and implements display_info
        Result: TicketAgent.display_info -> Person.display_info (transitive)

    Args:
        class_id: The class to find ancestors for
        inheritance: Map of child_id -> [parent_ids]
        visited: Set to prevent infinite loops in case of cycles

    Returns:
        List of all ancestor class IDs (direct + transitive)
    """
    if visited is None:
        visited = set()

    if class_id in visited:
        return []

    visited.add(class_id)
    ancestors = []

    for parent_id in inheritance.get(class_id, []):
        ancestors.append(parent_id)
        ancestors.extend(build_all_ancestors(parent_id, inheritance, visited))

    return ancestors


def build_inheritance_map(conn: sqlite3.Connection) -> Dict[bytes, List[bytes]]:
    """Build inheritance map from Extend dependencies in database."""
    cursor = conn.cursor()
    cursor.execute("SELECT src, tgt FROM deps WHERE kind = 'Extend'")

    inheritance: Dict[bytes, List[bytes]] = defaultdict(list)
    for child_id, parent_id in cursor.fetchall():
        inheritance[child_id].append(parent_id)

    return inheritance


# =============================================================================
# Language Detection
# =============================================================================

def detect_language(conn: sqlite3.Connection) -> str:
    """
    Detect programming language from file extensions in database.

    Returns:
        'python', 'java', or 'unknown'
    """
    cursor = conn.cursor()
    cursor.execute("SELECT name FROM entities WHERE kind = 'File' LIMIT 20")
    files = [row[0] for row in cursor.fetchall()]

    python_count = sum(1 for f in files if f.endswith('.py'))
    java_count = sum(1 for f in files if f.endswith('.java'))

    if python_count > java_count:
        return "python"
    elif java_count > 0:
        return "java"
    return "unknown"


# =============================================================================
# Python Override Detection
# =============================================================================

class PythonClassAnalyzer(ast.NodeVisitor):
    """
    AST visitor to analyze Python classes.

    Extracts:
    - Class name
    - Base class names
    - Method names
    - Abstract method names (methods with @abstractmethod decorator)
    """

    def __init__(self):
        self.classes: Dict[str, Dict] = {}

    def visit_ClassDef(self, node: ast.ClassDef):
        """Visit class definition to extract inheritance and methods."""
        class_info = {
            'name': node.name,
            'bases': [],
            'methods': set(),
            'abstract_methods': set()
        }

        # Extract base classes
        for base in node.bases:
            if isinstance(base, ast.Name):
                class_info['bases'].append(base.id)
            elif isinstance(base, ast.Attribute):
                class_info['bases'].append(base.attr)

        # Extract methods and check for @abstractmethod
        for item in node.body:
            if isinstance(item, ast.FunctionDef):
                class_info['methods'].add(item.name)

                for decorator in item.decorator_list:
                    if isinstance(decorator, ast.Name) and decorator.id == "abstractmethod":
                        class_info['abstract_methods'].add(item.name)
                    elif isinstance(decorator, ast.Attribute) and decorator.attr == "abstractmethod":
                        class_info['abstract_methods'].add(item.name)

        self.classes[node.name] = class_info
        self.generic_visit(node)


def analyze_python_file(file_content: str) -> Dict[str, Dict]:
    """Parse Python file and analyze all classes."""
    try:
        tree = ast.parse(file_content)
        analyzer = PythonClassAnalyzer()
        analyzer.visit(tree)
        return analyzer.classes
    except SyntaxError:
        return {}


def add_python_extend_dependencies(conn: sqlite3.Connection) -> int:
    """
    Extract inheritance relationships from Python AST and add Extend dependencies.
    Only runs if no Extend dependencies exist (to avoid duplicates with Depends mode).
    """
    print("  Checking for existing Extend dependencies...")

    cursor = conn.cursor()
    cursor.execute("SELECT COUNT(*) FROM deps WHERE kind = 'Extend'")
    existing_count = cursor.fetchone()[0]

    if existing_count > 0:
        print(f"    Found {existing_count} existing Extend dependencies (from NeoDepends)")
        print("    Skipping AST extraction to avoid duplicates")
        return 0

    print("    No Extend dependencies found, extracting from Python AST...")

    all_classes = get_all_classes(conn)
    extends_to_add: List[Tuple[bytes, bytes]] = []

    for class_id, (class_name, content_id) in all_classes.items():
        file_content = get_file_content(content_id, conn)
        if not file_content:
            continue

        class_info = analyze_python_file(file_content)
        if class_name not in class_info:
            continue

        for base_name in class_info[class_name]['bases']:
            if base_name == 'ABC':
                continue

            parent_id = get_class_by_name(conn, base_name, content_id)
            if not parent_id:
                for pid, (pname, _) in all_classes.items():
                    if pname == base_name:
                        parent_id = pid
                        break

            if parent_id:
                extends_to_add.append((class_id, parent_id))
                print(f"    Found: {class_name} extends {base_name}")

    print(f"  Inserting {len(extends_to_add)} Extend dependencies...")
    for child_id, parent_id in extends_to_add:
        cursor.execute("""
            INSERT INTO deps (src, tgt, kind, row, commit_id)
            VALUES (?, ?, 'Extend', 0, NULL)
        """, (child_id, parent_id))

    conn.commit()
    return len(extends_to_add)


def detect_python_overrides(conn: sqlite3.Connection, source_root: str) -> int:
    """
    Detect Python @abstractmethod overrides and add Override dependencies.

    Strategy:
    1. Build inheritance tree from Extend edges
    2. Parse Python AST to find @abstractmethod decorators
    3. Build transitive closure of ancestors
    4. Find child methods that override abstract parent methods
    5. Insert Override edges
    """
    print("\nStep 1: Processing Python inheritance...")
    add_python_extend_dependencies(conn)

    print("\nStep 2: Detecting @abstractmethod overrides (direct + transitive)...")

    cursor = conn.cursor()
    inheritance = build_inheritance_map(conn)
    print(f"  Found {len(inheritance)} classes with inheritance")

    all_classes = get_all_classes(conn)
    abstract_methods: Dict[bytes, Set[str]] = {}

    for class_id, (class_name, content_id) in all_classes.items():
        file_content = get_file_content(content_id, conn)
        if not file_content:
            continue

        class_info = analyze_python_file(file_content)
        if class_name in class_info and class_info[class_name]['abstract_methods']:
            abstract_methods[class_id] = class_info[class_name]['abstract_methods']
            print(f"    Class {class_name} has abstract methods: {class_info[class_name]['abstract_methods']}")

    print(f"  Found {len(abstract_methods)} classes with abstract methods")

    print("  Detecting override relationships...")
    overrides_to_add: List[Tuple[bytes, bytes]] = []
    seen_overrides: Set[Tuple[bytes, bytes]] = set()

    for child_class_id in all_classes.keys():
        child_class_name = all_classes[child_class_id][0]
        child_methods = get_class_methods(conn, child_class_id)

        if not child_methods:
            continue

        all_ancestors = build_all_ancestors(child_class_id, inheritance)

        for ancestor_id in all_ancestors:
            if ancestor_id not in abstract_methods:
                continue

            ancestor_class_name = all_classes[ancestor_id][0]
            ancestor_methods = get_class_methods(conn, ancestor_id)

            for abstract_method_name in abstract_methods[ancestor_id]:
                if abstract_method_name in child_methods and abstract_method_name in ancestor_methods:
                    child_method_id = child_methods[abstract_method_name]
                    ancestor_method_id = ancestor_methods[abstract_method_name]

                    edge = (child_method_id, ancestor_method_id)
                    if edge not in seen_overrides:
                        seen_overrides.add(edge)
                        overrides_to_add.append(edge)

                        is_direct = ancestor_id in inheritance.get(child_class_id, [])
                        override_type = "direct" if is_direct else "transitive"
                        print(f"    Override ({override_type}): {child_class_name}.{abstract_method_name} -> {ancestor_class_name}.{abstract_method_name}")

    print(f"  Inserting {len(overrides_to_add)} Override dependencies...")
    for child_method_id, parent_method_id in overrides_to_add:
        cursor.execute("""
            INSERT INTO deps (src, tgt, kind, row, commit_id)
            VALUES (?, ?, 'Override', 0, NULL)
        """, (child_method_id, parent_method_id))

    conn.commit()
    return len(overrides_to_add)


# =============================================================================
# Java Override Detection
# =============================================================================

def find_java_override_methods(source: str) -> List[str]:
    """
    Extract method names that have @Override annotation.

    Uses regex to find @Override followed by method signature.
    """
    # Pattern: @Override followed by optional modifiers, return type, method name, and (
    # Handles multiline with \s+ (one or more whitespace including newlines)
    pattern = r'@Override\s+(?:public\s+|protected\s+|private\s+)?(?:static\s+)?(?:final\s+)?(?:synchronized\s+)?(?:\w+(?:<[^>]+>)?(?:\[\])?)\s+(\w+)\s*\('
    matches = re.findall(pattern, source, re.MULTILINE)
    return matches


def detect_java_overrides(conn: sqlite3.Connection, source_root: str) -> int:
    """
    Detect Java @Override annotations and add Override dependencies.

    Strategy:
    1. Build inheritance tree from existing Extend edges
    2. Parse Java source for @Override annotations
    3. Find parent method being overridden by walking up inheritance tree
    4. Insert Override edges

    Java is simpler than Python because @Override explicitly marks which methods
    are overrides - we don't need to check if parent method is abstract.
    """
    print("\nStep 1: Building Java inheritance tree...")

    cursor = conn.cursor()
    inheritance = build_inheritance_map(conn)
    print(f"  Found {len(inheritance)} classes with inheritance")

    if not inheritance:
        print("  No inheritance found - nothing to do")
        return 0

    print("\nStep 2: Detecting @Override annotations...")

    all_classes = get_all_classes(conn)
    overrides_to_add: List[Tuple[bytes, bytes]] = []
    seen_overrides: Set[Tuple[bytes, bytes]] = set()

    for class_id, (class_name, content_id) in all_classes.items():
        file_content = get_file_content(content_id, conn)
        if not file_content:
            continue

        # Find methods with @Override annotation
        override_method_names = find_java_override_methods(file_content)
        if not override_method_names:
            continue

        child_methods = get_class_methods(conn, class_id)
        if not child_methods:
            continue

        # Get all ancestors
        all_ancestors = build_all_ancestors(class_id, inheritance)
        if not all_ancestors:
            continue

        for method_name in override_method_names:
            if method_name not in child_methods:
                continue

            child_method_id = child_methods[method_name]

            # Find the ancestor that defines this method
            for ancestor_id in all_ancestors:
                if ancestor_id not in all_classes:
                    continue

                ancestor_class_name = all_classes[ancestor_id][0]
                ancestor_methods = get_class_methods(conn, ancestor_id)

                if method_name in ancestor_methods:
                    ancestor_method_id = ancestor_methods[method_name]

                    edge = (child_method_id, ancestor_method_id)
                    if edge not in seen_overrides:
                        seen_overrides.add(edge)
                        overrides_to_add.append(edge)

                        is_direct = ancestor_id in inheritance.get(class_id, [])
                        override_type = "direct" if is_direct else "transitive"
                        print(f"    Override ({override_type}): {class_name}.{method_name} -> {ancestor_class_name}.{method_name}")
                    break  # Only link to first ancestor with this method

    print(f"  Inserting {len(overrides_to_add)} Override dependencies...")
    for child_method_id, parent_method_id in overrides_to_add:
        cursor.execute("""
            INSERT INTO deps (src, tgt, kind, row, commit_id)
            VALUES (?, ?, 'Override', 0, NULL)
        """, (child_method_id, parent_method_id))

    conn.commit()
    return len(overrides_to_add)


# =============================================================================
# Main Entry Point
# =============================================================================

def detect_overrides(db_path: str, source_root: str) -> int:
    """
    Main entry point - detects language and dispatches to appropriate handler.

    Returns:
        Number of Override dependencies added
    """
    conn = sqlite3.Connection(db_path)

    print(f"Analyzing database: {db_path}")
    print(f"Source root: {source_root}")
    print()

    lang = detect_language(conn)
    print(f"Detected language: {lang}")

    if lang == "python":
        override_count = detect_python_overrides(conn, source_root)
    elif lang == "java":
        override_count = detect_java_overrides(conn, source_root)
    else:
        print(f"Override detection not supported for language: {lang}")
        override_count = 0

    conn.close()
    return override_count


def main():
    parser = argparse.ArgumentParser(
        description="Detect method overrides for Python and Java"
    )
    parser.add_argument(
        "database",
        help="Path to NeoDepends SQLite database (deps.db)"
    )
    parser.add_argument(
        "source_root",
        help="Root directory of source code"
    )

    args = parser.parse_args()

    db_path = Path(args.database)
    if not db_path.exists():
        print(f"Error: Database not found: {db_path}", file=sys.stderr)
        return 1

    source_root = Path(args.source_root)
    if not source_root.exists():
        print(f"Error: Source root not found: {source_root}", file=sys.stderr)
        return 1

    try:
        override_count = detect_overrides(str(db_path), str(source_root))
        print(f"\nDone!")
        print(f"  Added {override_count} Override dependencies")
        return 0
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        import traceback
        traceback.print_exc()
        return 1


if __name__ == "__main__":
    sys.exit(main())
