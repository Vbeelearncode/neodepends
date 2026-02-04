# PyInstaller spec file for neodepends-analyze
# This compiles run_dependency_analysis.py into a standalone executable
# that bundles the entire neodepends_python_export pipeline

# -*- mode: python ; coding: utf-8 -*-

import os
from pathlib import Path

block_cipher = None

# Get the tools directory to bundle neodepends_python_export
spec_dir = Path(SPECPATH)
tools_dir = spec_dir.parent / 'tools'

a = Analysis(
    ['../run_dependency_analysis.py'],
    pathex=[str(tools_dir)],  # Add tools directory to search path
    binaries=[],
    datas=[
        # Bundle all required Python scripts from tools directory
        (str(tools_dir / 'neodepends_python_export.py'), 'tools'),
        (str(tools_dir / 'filter_false_positives.py'), 'tools'),
        (str(tools_dir / 'enhance_python_deps.py'), 'tools'),
        (str(tools_dir / 'export_dv8_from_neodepends_db.py'), 'tools'),
        (str(tools_dir / 'detect_overrides.py'), 'tools'),
    ],
    hiddenimports=[
        'neodepends_python_export',  # Explicitly include the export module
    ],
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[],
    win_no_prefer_redirects=False,
    win_private_assemblies=False,
    cipher=block_cipher,
    noarchive=False,
)

pyz = PYZ(a.pure, a.zipped_data, cipher=block_cipher)

exe = EXE(
    pyz,
    a.scripts,
    a.binaries,
    a.zipfiles,
    a.datas,
    [],
    name='dependency-analyzer',
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    upx_exclude=[],
    runtime_tmpdir=None,
    console=True,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
)
