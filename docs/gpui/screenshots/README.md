# Desktop comparison captures

These images came from the running macOS applications through the native screenshot tool.
They contain the [authored parity project](../fixtures/README.md) and use isolated configuration and recovery directories.
Provider responses came from the [offline ACP fixture](../fixture-adapter.md).
No live model account was used.

The screenshot set is incomplete. The native control service now returns `cgWindowNotFound` for both applications, including after a GPUI restart.
The remaining GPUI captures and OS picker/drop checks need a working desktop session.
The [parity matrix](../parity.md) records the checks that remain open.
These images do not establish full feature parity.

## Search palette

Open the fixture, press Cmd+K, and enter `Native rich answer`.
Both frontends show the same two matching nodes in the same order.
The Tauri capture keeps the rich preview open behind the palette.
The GPUI capture has the preview closed.

| Tauri | GPUI |
| --- | --- |
| ![Tauri search palette](tauri-palette.jpg) | ![Native GPUI search palette](gpui-palette.jpg) |

The original captures are 4112×2580 pixels for Tauri and 2880×1946 pixels for GPUI, including native window chrome.
No pixels were edited. The file extensions match the JPEG bytes returned by the screenshot service.

## Tauri reference states

The following reference captures need matching GPUI images before visual acceptance is complete.
Except for the project chooser, they use the same maximized window as the Tauri palette capture.

| State and action | Reference |
| --- | --- |
| Start with an isolated configured Vault. The chooser lists the fixture project. This window is 1600×1200 pixels. | ![Tauri project chooser](tauri-projects.jpg) |
| Open the fixture and fit the graph. This capture follows one fixture generation, so it has 12 nodes instead of the original 11. | ![Tauri graph](tauri-graph.jpg) |
| Open `parity-synthesis`. Inspect the table, Rust code, formulas, diagram, and task list. | ![Tauri rich answer](tauri-rich-answer.jpg) |
| Expand Provenance and the Read activity. Inspect reference indexes, the partial-evidence warning, and the missing citation. | ![Tauri provenance](tauri-provenance.jpg) |
| Open Settings. Inspect provider availability, executable configuration, model selection, and project effort. | ![Tauri provider settings](tauri-settings.jpg) |
| Open the user node `parity-image` and choose Edit. Inspect the source and attachment thumbnail. | ![Tauri user editor](tauri-editor.jpg) |
| Open the linked Markdown file. Inspect its metadata and text preview. | ![Tauri file preview](tauri-file.jpg) |
| Generate with the offline fixture. The ACP turn pauses for its synthetic permission request. | ![Tauri ACP permission](tauri-permission.jpg) |
| Open Recovery snapshots after changing the isolated fixture. Inspect saved snapshots and source labels. | ![Tauri recovery](tauri-recovery.jpg) |

## Capture completion

Use the same copied fixture and comparable window sizes for the remaining pairs.
Reset the disposable project before the graph capture.
Capture the native rich answer after the SVG color conversion fix.
Also capture native Settings, editor, file preview, provenance, permission, recovery, and project chooser states.
Do not replace running-app captures with mockups or test-rendered images.
