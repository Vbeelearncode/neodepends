#!/usr/bin/env python3
"""
NeoDepends Dependency Analysis Runner
Cross-platform dependency analysis tool - works on Windows, macOS, and Linux

This script:
1. Checks for required dependencies (Python, neodepends binary, Java)
2. Prompts for analysis parameters
3. Runs the dependency analysis pipeline

This binary bundles the entire neodepends Python export pipeline internally.
"""

import os
import sys
import subprocess
import platform
from pathlib import Path

# Add tools directory to path so we can import neodepends_python_export
sys.path.insert(0, str(Path(__file__).parent / "tools"))


def print_header():
    """Print welcome header"""
    print()
    print("=" * 70)
    print("NeoDepends - Dependency Analysis Tool")
    print("=" * 70)
    print()
    print(f"Platform: {platform.system()} {platform.release()}")
    print(f"Architecture: {platform.machine()}")
    print(f"Python: {sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}")
    print()


def check_python_version():
    """Check if Python version is 3.7 or higher"""
    if sys.version_info < (3, 7):
        print("[ERROR] Python 3.7 or higher is required")
        print(f"        Current version: {sys.version}")
        return False
    return True


def find_neodepends_binary():
    """Find the neodepends-core binary in common locations"""
    script_dir = Path(__file__).parent.resolve()

    if platform.system() == "Windows":
        candidates = [
            script_dir / "bin" / "neodepends-core.exe",  # Release bundle location
            script_dir / "target" / "release" / "neodepends.exe",  # Development
            "bin/neodepends-core.exe",
            "neodepends.exe"  # Fallback
        ]
        default = "./bin/neodepends-core.exe"
    else:
        candidates = [
            script_dir / "bin" / "neodepends-core",  # Release bundle location
            script_dir / "target" / "release" / "neodepends",  # Development
            "bin/neodepends-core",
            "neodepends"  # Fallback
        ]
        default = "./bin/neodepends-core"

    # Check if any candidate exists
    for candidate in candidates:
        if Path(candidate).exists():
            return str(candidate), True

    return default, False


def check_java():
    """Check if Java is installed (needed for Java dependency analysis)"""
    try:
        result = subprocess.run(
            ["java", "-version"],
            check=True,
            capture_output=True,
            text=True
        )
        version_output = result.stderr.split('\n')[0] if result.stderr else "unknown"
        return True, version_output
    except (subprocess.CalledProcessError, FileNotFoundError):
        return False, None


def run_dependency_checks():
    """Run all dependency checks and report status"""
    print("Checking dependencies...")
    print()

    all_ok = True

    # Check Python version
    if not check_python_version():
        print("[FAILED] Python version check")
        return False
    print("[OK] Python version")

    # Check for neodepends-core binary
    neodepends_path, found = find_neodepends_binary()
    if found:
        print(f"[OK] NeoDepends core binary found: {neodepends_path}")
    else:
        print("[ERROR] NeoDepends core binary not found")
        print()
        print("Please ensure the neodepends-core binary is in one of these locations:")
        print("  - bin/neodepends-core")
        print("  - ./target/release/neodepends (development)")
        print()
        print("To get the binary:")
        print("  1. Download a release bundle from GitHub, or")
        print("  2. Build from source: cargo build --release")
        print()
        return False

    # Check for Java (optional)
    java_available, java_version = check_java()
    if java_available:
        print(f"[OK] Java available: {java_version}")
    else:
        print("[INFO] Java not found - Java dependency analysis will not be available")
        print("       (Only needed if analyzing Java projects)")

    print()
    print("-" * 70)
    print()

    return neodepends_path


def run_analysis(neodepends_bin):
    """Run the interactive dependency analysis"""
    script_dir = Path(__file__).parent.resolve()
    os.chdir(script_dir)

    print("Dependency Analysis Configuration")
    print()

    # Prompt for neodepends binary path (with default)
    print(f"NeoDepends binary: {neodepends_bin}")
    custom_path = input("Press Enter to use this path, or enter a custom path: ").strip()
    if custom_path:
        neodepends_bin = custom_path
        if not Path(neodepends_bin).exists():
            print(f"[ERROR] Binary not found at: {neodepends_bin}")
            return False

    print()

    # Prompt for input repository
    input_repo = input("Enter input repository path: ").strip()
    if not input_repo:
        print("[ERROR] Input repository path cannot be empty")
        return False
    if not Path(input_repo).exists():
        print(f"[ERROR] Input repository path does not exist: {input_repo}")
        return False

    # Prompt for output location
    output_dir = input("Enter output directory path: ").strip()
    if not output_dir:
        print("[ERROR] Output directory cannot be empty")
        return False

    # Create output directory if it doesn't exist
    Path(output_dir).mkdir(parents=True, exist_ok=True)

    # Prompt for language
    language = input("Enter language (python or java): ").strip().lower()
    if language not in ("python", "java"):
        print("[ERROR] Language must be 'python' or 'java'")
        return False

    # Auto-select resolver based on language
    if language == "python":
        model = "stackgraphs"
        print("Auto-selected resolver: stackgraphs (for Python)")
    elif language == "java":
        model = "depends"
        print("Auto-selected resolver: depends (for Java)")

        # Check Java availability for Java analysis
        java_available, _ = check_java()
        if not java_available:
            print()
            print("[ERROR] Java is required for Java dependency analysis")
            print("        Please install Java 11 or higher and try again")
            return False
    else:
        print(f"[ERROR] Unsupported language: {language}")
        return False

    # Build the command arguments for the export pipeline
    args = [
        "--neodepends-bin", neodepends_bin,
        "--input", input_repo,
        "--output-dir", output_dir,
        "--langs", language,
        "--resolver", model,
        "--dv8-hierarchy", "structured",
        "--filter-architecture"
    ]

    # For Python with stackgraphs, add stackgraphs-specific flags
    if language == "python" and model == "stackgraphs":
        args.extend([
            "--stackgraphs-python-mode", "ast",
            "--filter-stackgraphs-false-positives"
        ])

    print()
    print("=" * 70)
    print("Running dependency analysis with the following settings:")
    print(f"  NeoDepends binary: {neodepends_bin}")
    print(f"  Input repository: {input_repo}")
    print(f"  Output directory: {output_dir}")
    print(f"  Language: {language}")
    print(f"  Resolver: {model}")
    print("=" * 70)
    print()

    # Import and run the export pipeline directly (bundled in this binary)
    try:
        import neodepends_python_export
        # Save original sys.argv and replace with our args
        original_argv = sys.argv
        sys.argv = ["neodepends_python_export"] + args

        # Call the main function
        exit_code = neodepends_python_export.main()

        # Restore original sys.argv
        sys.argv = original_argv

        if exit_code != 0:
            print()
            print(f"[ERROR] Analysis failed with exit code {exit_code}")
            return False
    except Exception as e:
        print()
        print(f"[ERROR] Analysis failed: {e}")
        import traceback
        traceback.print_exc()
        return False

    # Determine the output filename based on resolver
    if model == "depends":
        resolver_name = "depends"
    elif model == "stackgraphs" and language == "python":
        resolver_name = "stackgraphs_ast"
    else:
        resolver_name = "stackgraphs"

    output_file = Path(output_dir) / f"dependencies.{resolver_name}.filtered.dv8-dsm-v3.json"

    print()
    print("=" * 70)
    print("Dependency analysis complete!")
    print()
    print(f"Results saved to: {output_dir}")
    print()
    print("To visualize results in DV8 Explorer, open:")
    print(f"  {output_file}")
    print("=" * 70)
    print()

    return True


def main():
    print_header()

    # Run dependency checks
    neodepends_bin = run_dependency_checks()
    if not neodepends_bin:
        print("[FAILED] Dependency checks failed")
        print("Please resolve the issues above and try again")
        sys.exit(1)

    print("All dependency checks passed!")
    print()

    # Run analysis
    if not run_analysis(neodepends_bin):
        sys.exit(1)


if __name__ == "__main__":
    main()
