#!/usr/bin/env python3
"""
Test suite for Nudge Linux Desktop Integration & Installer
"""

import os
import sys
import subprocess
from pathlib import Path

# Add parent directory to sys.path to import NdgAppInfo and NudgeAppManager
SCRIPT_DIR = Path(__file__).resolve().parent
PARENT_DIR = SCRIPT_DIR.parent
sys.path.insert(0, str(PARENT_DIR))

from importlib.machinery import SourceFileLoader
installer_module = SourceFileLoader("nudge_app_installer", str(PARENT_DIR / "nudge-app-installer")).load_module()

NdgAppInfo = installer_module.NdgAppInfo
NudgeAppManager = installer_module.NudgeAppManager

def test_metadata_parsing():
    nudge_root = PARENT_DIR.parent.parent
    examples_dir = nudge_root / "examples"

    # Test 1: GUI demo
    gui_file = examples_dir / "theme_toggle.ndg"
    assert gui_file.exists(), f"{gui_file} does not exist"
    app_gui = NdgAppInfo(gui_file)
    assert app_gui.is_gui is True, "theme_toggle.ndg must be recognized as GUI"
    assert "UI" in app_gui.effects, "UI effect must be present"
    assert "Theme" in app_gui.title, f"Title should contain Theme: {app_gui.title}"

    # Test 2: CLI / Agent demo
    cli_file = examples_dir / "chatbot" / "chatbot.ndg"
    assert cli_file.exists(), f"{cli_file} does not exist"
    app_cli = NdgAppInfo(cli_file)
    assert app_cli.is_gui is False, "chatbot.ndg must be recognized as CLI"
    assert "LLM" in app_cli.effects, "LLM effect must be present"

    print("✓ test_metadata_parsing passed!")

def test_install_and_uninstall_lifecycle():
    nudge_root = PARENT_DIR.parent.parent
    test_file = nudge_root / "examples" / "hello_llm.ndg"
    assert test_file.exists()

    app = NdgAppInfo(test_file)
    # Ensure fresh state
    if app.is_installed:
        NudgeAppManager.uninstall(app)

    assert not app.is_installed, "Should not be installed initially"

    # Install
    ok = NudgeAppManager.install(app)
    assert ok is True, "Install should succeed"
    assert app.is_installed is True, "App should report as installed"
    assert app.desktop_file_path.exists(), f"Desktop file {app.desktop_file_path} must exist"
    assert app.installed_executable_path.exists(), f"Binary {app.installed_executable_path} must exist"
    assert os.access(app.installed_executable_path, os.X_OK), "Executable must be executable (+x)"

    # Check desktop file content
    content = app.desktop_file_path.read_text()
    assert "Type=Application" in content
    assert "MimeType=application/x-nudge;" in content

    # Uninstall
    ok = NudgeAppManager.uninstall(app)
    assert ok is True, "Uninstall should succeed"
    assert not app.desktop_file_path.exists(), "Desktop file must be deleted"
    assert not app.installed_executable_path.exists(), "Binary wrapper must be deleted"

    print("✓ test_install_and_uninstall_lifecycle passed!")

def test_mime_type_association():
    nudge_root = PARENT_DIR.parent.parent
    sample_ndg = nudge_root / "examples" / "simple_add.ndg"
    res = subprocess.run(["xdg-mime", "query", "filetype", str(sample_ndg)], capture_output=True, text=True)
    assert "application/x-nudge" in res.stdout, f"Unexpected MIME type: {res.stdout}"

    handler = subprocess.run(["xdg-mime", "query", "default", "application/x-nudge"], capture_output=True, text=True)
    assert "nudge-app-installer.desktop" in handler.stdout, f"Unexpected default handler: {handler.stdout}"

    print("✓ test_mime_type_association passed!")

if __name__ == "__main__":
    print("Testing Nudge Linux Desktop Integration...")
    test_metadata_parsing()
    test_install_and_uninstall_lifecycle()
    test_mime_type_association()
    print("All tests completed successfully!")
