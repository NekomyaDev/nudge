# KDE Dolphin File Manager — Pure-Nudge Edition (`dolphin.ndg`)

> **Substrate:** 100% Pure Nudge Language (`.ndg`)  
> **Foreign Dependencies:** 0 (No Python, No C, No Electron, No External Scripts)  
> **Design Pattern:** KDE Dolphin Desktop Specification (KDE HIG Compliant)  
> **Engine:** Pack 3 Local UI (Softbuffer/Winit) + In-Memory NVFS Caching

---

## 1. Overview

**Dolphin-NDG** is a desktop file manager implemented in pure Nudge. It merges the professional desktop UX of **KDE Dolphin** with Nudge's self-hosted **In-Memory Virtual File System (NVFS)**.

By leveraging RAM-resident Inode tables and Content-Addressable Storage (CAS) deduplication, Dolphin-NDG delivers:
* **Zero Disk I/O After Cold Start:** Directories once viewed are retained in NVFS memory, yielding $> 100\times$ faster navigation and instant filtering.
* **Deterministic Headless Verification:** The entire user interface, navigation state, dual-panel split, clipboard, and inspector can be tested in CI without an X11 or Wayland display.
* **100% Pure Nudge:** Zero foreign bindings, ensuring cross-platform confinement within the Nudge sandbox.

---

## 2. Key Features Aligned with KDE Dolphin

| Dolphin Feature | Implementation in `dolphin.ndg` | Status |
| :--- | :--- | :--- |
| **Interactive Breadcrumb Bar** | Path segments rendered as clickable buttons with a toggle to raw input mode | **Complete** |
| **Places Sidebar** | Categorized quick-access tree: *Places*, *Devices*, and *Bookmarks* | **Complete** |
| **Split View (`F3`)** | Dual-panel workspace (`Panel Left` & `Panel Right`) with independent directory states | **Complete** |
| **Information Inspector (`F11`)** | Right-hand inspector displaying metadata, permissions, CAS hashes, and `code_block` syntax preview | **Complete** |
| **Live Filter Bar (`Ctrl+I`)** | Instant substring and extension filtering backed by in-memory posting lists | **Complete** |
| **Directory-First Sorting** | Dolphin HIG rule: Folders always group at the top regardless of sort criterion | **Complete** |
| **Clipboard & File Ops** | In-memory clipboard (`Copy`, `Paste`) and atomic folder creation | **Complete** |
| **Status Bar** | Real-time counts of folders and files, selection stats, and NVFS cache hit metrics | **Complete** |

---

## 3. Running & Verifying Dolphin-NDG

### A. Type-Check the Application
Verify that all 27 items, effects (`uses UI, Workspace, WorkspaceDelete`), and types pass Nudge's strict compiler checks:
```bash
cargo run -p nudgec --bin nudgec -- check examples/dolphin-fm/dolphin.ndg
# Output: -- checked 27 item(s): OK
```

### B. Run the Headless Acceptance Test Suite
Execute the deterministic test suite:
```bash
cargo run -p nudgec --bin nudgec -- test examples/dolphin-fm/dolphin.ndg
```
Expected output:
```
PASS nudge_test_clipboard_copy_stores_selected_file_in_buffer
PASS nudge_test_dolphin_app_mounts_with_professional_kde_layout
PASS nudge_test_inspector_f11_toggles_display_and_updates_metadata
PASS nudge_test_live_filter_bar_updates_visible_entries_via_substring_search
PASS nudge_test_sorting_groups_directories_first_and_updates_order
PASS nudge_test_split_view_f3_toggles_dual_panels_and_synchronizes_entries
6/6 tests passed
```

### C. Launch on the Bytecode VM
Run the master application entrypoint:
```bash
cargo run -p nudgec --bin nudgec -- run examples/dolphin-fm/dolphin.ndg
```
