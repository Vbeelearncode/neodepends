#!/usr/bin/env python3
"""
Post-processing script to filter out false positive dependencies caused by
stack-graphs class scope bleed in Python code.

This script reads a database with dependencies and creates a new cleaned database
with false positives removed.

Usage: python3 filter_false_positives.py <input_db> <output_db>
Example: python3 filter_false_positives.py output.db output_cleaned.db
"""

import sqlite3
import sys
import shutil
from pathlib import Path


def is_false_positive_sibling_method(cursor: sqlite3.Cursor, src_id: bytes, tgt_id: bytes, dep_row: int) -> bool:
    """
    Detect false positive: Method depending on sibling method at definition area.

    This occurs when stack-graphs processes type annotations or method signatures
    and incorrectly creates dependencies to all methods in the same class.

    For simple methods (def + single statement), false positives can appear at either:
    - start_row (the def line with type annotations)
    - end_row (which tree-sitter may attribute to the signature)
    """
    # Get source entity info
    cursor.execute("""
        SELECT kind, parent_id, start_row, end_row
        FROM entities
        WHERE id = ?
    """, (src_id,))
    src_result = cursor.fetchone()

    if not src_result:
        return False

    src_kind, src_parent_id, src_start_row, src_end_row = src_result

    # Source must be a Method
    if src_kind != 'Method':
        return False

    # Get target entity info
    cursor.execute("""
        SELECT kind, parent_id
        FROM entities
        WHERE id = ?
    """, (tgt_id,))
    tgt_result = cursor.fetchone()

    if not tgt_result:
        return False

    tgt_kind, tgt_parent_id = tgt_result

    # Target must be a Method
    if tgt_kind != 'Method':
        return False

    # Must be siblings (same parent)
    if src_parent_id != tgt_parent_id:
        return False

    # Dependencies at the method definition lines are suspicious
    # - start_row: type annotations in signature
    # - end_row: closing of method definition (tree-sitter artifact)
    #
    # For very small methods (1-3 lines), both start and end are suspect
    # For larger methods, end_row is where tree-sitter often places spurious deps
    method_size = src_end_row - src_start_row + 1

    if method_size <= 3:
        # Small methods: check both start and end
        if dep_row == src_start_row or dep_row == src_end_row:
            return True
    else:
        # Larger methods: end_row is more suspicious (method closing)
        # start_row can have legitimate dependencies (e.g., decorators, multi-line signatures)
        # But sibling method dependencies at end_row are almost certainly false positives
        if dep_row == src_end_row:
            return True
        # Also check start_row for very suspicious patterns
        if dep_row == src_start_row:
            return True

    return False


def is_false_positive_parent_class(cursor: sqlite3.Cursor, src_id: bytes, tgt_id: bytes, dep_row: int) -> bool:
    """
    Detect false positive: Method depending on its parent class at definition area.

    This occurs when stack-graphs processes method signatures and creates spurious
    dependencies to the parent class.
    """
    # Get source entity info
    cursor.execute("""
        SELECT kind, parent_id, start_row, end_row
        FROM entities
        WHERE id = ?
    """, (src_id,))
    src_result = cursor.fetchone()

    if not src_result:
        return False

    src_kind, src_parent_id, src_start_row, src_end_row = src_result

    # Source must be a Method
    if src_kind != 'Method':
        return False

    # Get target entity info
    cursor.execute("""
        SELECT kind
        FROM entities
        WHERE id = ?
    """, (tgt_id,))
    tgt_result = cursor.fetchone()

    if not tgt_result:
        return False

    tgt_kind = tgt_result[0]

    # Target must be a Class
    if tgt_kind != 'Class':
        return False

    # Target must be the parent of source
    if src_parent_id != tgt_id:
        return False

    # Method-to-parent-class dependencies at definition lines
    # These occur from stack-graphs class scope lookups
    method_size = src_end_row - src_start_row + 1

    if method_size <= 3:
        # Small methods: both start and end are suspect
        if dep_row == src_start_row or dep_row == src_end_row:
            return True
    else:
        # Larger methods: end_row is more common for spurious parent deps
        if dep_row == src_end_row:
            return True
        # Also check start_row
        if dep_row == src_start_row:
            return True

    return False


def is_false_positive_field_sibling(cursor: sqlite3.Cursor, src_id: bytes, tgt_id: bytes, dep_row: int) -> bool:
    """
    Detect false positive: Field depending on sibling method at definition area.

    Similar to method-to-method false positives, but for fields.
    """
    # Get source entity info
    cursor.execute("""
        SELECT kind, parent_id, start_row, end_row
        FROM entities
        WHERE id = ?
    """, (src_id,))
    src_result = cursor.fetchone()

    if not src_result:
        return False

    src_kind, src_parent_id, src_start_row, src_end_row = src_result

    # Source must be a Field
    if src_kind != 'Field':
        return False

    # Get target entity info
    cursor.execute("""
        SELECT kind, parent_id
        FROM entities
        WHERE id = ?
    """, (tgt_id,))
    tgt_result = cursor.fetchone()

    if not tgt_result:
        return False

    tgt_kind, tgt_parent_id = tgt_result

    # Target must be a Method in the same parent
    if tgt_kind != 'Method':
        return False

    if src_parent_id != tgt_parent_id:
        return False

    # Dependency at field definition line or nearby
    if dep_row == src_start_row or dep_row == src_end_row:
        return True

    return False


def filter_dependencies(input_db: str, output_db: str):
    """
    Filter false positive dependencies and create a cleaned database.
    """
    # Create a copy of the database
    print(f"Copying database from {input_db} to {output_db}...")
    shutil.copy2(input_db, output_db)

    # Connect to the new database
    conn = sqlite3.connect(output_db)
    cursor = conn.cursor()

    # Get all dependencies
    print("Analyzing dependencies...")
    cursor.execute("SELECT src, tgt, kind, row FROM deps")
    all_deps = cursor.fetchall()

    total_deps = len(all_deps)
    false_positives = []

    print(f"Total dependencies: {total_deps}")
    print("Detecting false positives...")

    # Check each dependency
    for src, tgt, kind, row in all_deps:
        is_fp = False
        fp_reason = None

        # Check for sibling method false positive
        if is_false_positive_sibling_method(cursor, src, tgt, row):
            is_fp = True
            fp_reason = "sibling_method"

        # Check for parent class false positive
        elif is_false_positive_parent_class(cursor, src, tgt, row):
            is_fp = True
            fp_reason = "parent_class"

        # Check for field-method false positive
        elif is_false_positive_field_sibling(cursor, src, tgt, row):
            is_fp = True
            fp_reason = "field_to_method"

        if is_fp:
            false_positives.append((src, tgt, kind, row, fp_reason))

    print(f"Found {len(false_positives)} false positive dependencies")

    if false_positives:
        print("\nFalse Positives Breakdown:")

        # Count by reason
        from collections import Counter
        reasons = Counter(fp[4] for fp in false_positives)
        for reason, count in reasons.items():
            print(f"  {reason}: {count}")

        # Show some examples
        print("\nExamples of false positives being removed:")
        for i, (src, tgt, kind, row, reason) in enumerate(false_positives[:5]):
            cursor.execute("SELECT name, kind FROM entities WHERE id = ?", (src,))
            src_name, src_kind = cursor.fetchone()
            cursor.execute("SELECT name, kind FROM entities WHERE id = ?", (tgt,))
            tgt_name, tgt_kind = cursor.fetchone()
            print(f"  {i+1}. [{src_name} ({src_kind})] -> [{tgt_name} ({tgt_kind})] at row {row} (reason: {reason})")

        if len(false_positives) > 5:
            print(f"  ... and {len(false_positives) - 5} more")

        # Delete false positives
        print("\nRemoving false positives from database...")
        for src, tgt, kind, row, _ in false_positives:
            cursor.execute("""
                DELETE FROM deps
                WHERE src = ? AND tgt = ? AND kind = ? AND row = ?
            """, (src, tgt, kind, row))

        conn.commit()
        print(f"Removed {len(false_positives)} false positive dependencies")

    # Report final statistics
    cursor.execute("SELECT COUNT(*) FROM deps")
    final_count = cursor.fetchone()[0]

    print(f"\nFinal Statistics:")
    print(f"  Original dependencies: {total_deps}")
    print(f"  False positives removed: {len(false_positives)}")
    print(f"  Cleaned dependencies: {final_count}")
    if total_deps > 0:
        print(f"  Reduction: {len(false_positives) / total_deps * 100:.1f}%")
    else:
        print(f"  Reduction: N/A (no dependencies found)")

    conn.close()
    print(f"\nCleaned database saved to: {output_db}")


def main():
    if len(sys.argv) != 3:
        print("Usage: python3 filter_false_positives.py <input_db> <output_db>")
        print("Example: python3 filter_false_positives.py output.db output_cleaned.db")
        sys.exit(1)

    input_db = sys.argv[1]
    output_db = sys.argv[2]

    # Check if input exists
    if not Path(input_db).exists():
        print(f"Error: Input database '{input_db}' not found")
        sys.exit(1)

    # Check if output already exists
    if Path(output_db).exists():
        response = input(f"Warning: '{output_db}' already exists. Overwrite? (y/n): ")
        if response.lower() != 'y':
            print("Aborted.")
            sys.exit(0)

    filter_dependencies(input_db, output_db)


if __name__ == "__main__":
    main()
