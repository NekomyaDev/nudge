# Nudge File Manager (`file_manager.ndg`)

> **Substrate:** 100% Pure Nudge Language (`.ndg`)
> **Foreign Dependencies:** 0 (No Python, No C, No Electron, No External Scripts)
> **Design Pattern:** Dual-Pane Desktop File Manager (HIG Aligned)
> **Engine:** Pack 3 Local UI (Softbuffer/Winit) + In-Memory NVFS Caching

---

## 1. Overview

**Nudge File Manager** is a self-hosted desktop file manager implemented entirely in pure Nudge. It pairs a professional dual-pane desktop interface with Nudge's native **In-Memory Virtual File System (NVFS)**.

By leveraging RAM-resident Inode tables and Content-Addressable Storage (CAS) deduplication, Nudge File Manager delivers:
* **Zero Disk I/O After Cold Start:** Directories once viewed are retained in NVFS memory, yielding $> 100\times$ faster navigation and instant filtering.
* **Deterministic Headless Verification:** The entire user interface, navigation state, dual-panel split, clipboard, and inspector can be tested in CI without an X11 or Wayland display.
* **100% Pure Nudge:** Zero foreign bindings, ensuring cross-platform confinement within the Nudge sandbox.

---

## 2. Key Features

| Feature | Description | Status |
| :--- | :--- | :--- |
| **Interactive Breadcrumb Bar** | Path segments rendered as clickable buttons with a toggle to raw input mode | **Complete** |
| **Places Sidebar** | Categorized quick-access tree: *Places*, *Drives*, and *Bookmarks* | **Complete** |
| **Split View (`F3`)** | Dual-panel workspace (`Panel Left` & `Panel Right`) with independent directory states | **Complete** |
| **Information Inspector (`F11`)** | Right-hand inspector displaying metadata, permissions, CAS hashes, and `code_block` syntax preview | **Complete** |
| **Live Filter Bar (`Ctrl+I`)** | Instant substring and extension filtering backed by in-memory posting lists | **Complete** |
| **Directory-First Sorting** | Smart sorting rule: Folders always group at the top regardless of sort criterion | **Complete** |
| **Clipboard & File Ops** | In-memory clipboard (`Copy`, `Paste`) and atomic folder creation | **Complete** |
| **Transaction Undo** | Single-click rollback of newly created directories via in-memory journal | **Complete** |
| **Theme Toggle** | Native Light / Dark theme toggle (`Theme::LIGHT` and `Theme::DARK`) | **Complete** |
| **Status Bar** | Real-time counts of folders and files, selection stats, and NVFS cache hit metrics | **Complete** |

---

## 3. Running & Verifying Nudge File Manager

### A. Type-Check the Application
Verify that all items, effects (`uses UI, Workspace, WorkspaceDelete`), and types pass Nudge's strict compiler checks:
```bash
cargo run -p nudgec --bin nudgec -- check examples/file-manager/file_manager.ndg
# Output: -- checked 30 item(s): OK
```

### B. Run the Headless Acceptance Test Suite
Execute the deterministic test suite:
```bash
cargo run -p nudgec --bin nudgec -- test examples/file-manager/file_manager.ndg
```
Expected output:
```
PASS nudge_test_clipboard_copy_stores_selected_file_in_buffer
PASS nudge_test_file_manager_app_mounts_with_dual_pane_desktop_layout
PASS nudge_test_inspector_f11_toggles_display_and_updates_metadata
PASS nudge_test_live_filter_bar_updates_visible_entries_via_substring_search
PASS nudge_test_open_selected_directory_navigates_into_child_hierarchy
PASS nudge_test_sorting_groups_directories_first_and_updates_order
PASS nudge_test_split_view_f3_toggles_dual_panels_and_synchronizes_entries
PASS nudge_test_undo_action_rolls_back_newly_created_folder_transaction
8/8 tests passed
```

### C. Launch on the Native OS Desktop
Compile and run with native OS window support:
```bash
cargo run -p nudgec --features native-gui --bin nudgec -- run examples/file-manager/file_manager.ndg
```
