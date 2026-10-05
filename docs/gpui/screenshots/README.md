# Desktop comparison captures

The paired JPEG images came from the running macOS applications through the native screenshot tool.
The two PNG images under [Finder drop checks](#finder-drop-checks) were supplied by the user.
The native rich-answer PNG came from a Codex agent's Computer Use capture of the running application.
They contain the [authored parity project](../fixtures/README.md) and use isolated configuration and recovery directories.
Provider responses came from the [offline ACP fixture](../fixture-adapter.md).
No live model account was used.

Ten states now have captures from both frontends, with one additional native capture of blocked cards.
The user also supplied two images confirming real OS file and image drops.
The [parity matrix](../parity.md) records evidence for all 123 requirements.
These images show visible behavior. They do not establish full feature parity without the accompanying interaction and persistence checks.

The Tauri captures are 4112×2580 pixels, except for the 1600×1200 project chooser.
The GPUI JPEG captures are 2880×1946 pixels, and the native rich-answer PNG is 2880×1944 pixels. These dimensions include native window chrome.
The user-supplied PNGs show graph regions and measure 1134×450 and 1328×890 pixels.
All files are stored unchanged from the screenshot tool or user attachment. File extensions match the image bytes.

## Project chooser

Start with an isolated configured Vault. Open Recent and inspect the saved fixture entry.
Both captures show the project path and controls to browse, create, or import a project.
The native chooser also shows its Remove action and project filter.

| Tauri | GPUI |
| --- | --- |
| ![Tauri project chooser](tauri-projects.jpg) | ![Native GPUI project chooser](gpui-projects.jpg) |

## Search palette

Open the fixture and press Cmd+K. The Tauri query is `Native rich answer`; the native query is `native rich`.
Both queries show the same two matching nodes in the same order.
The Tauri capture keeps the rich preview open behind the palette.
The GPUI capture has the preview closed.

| Tauri | GPUI |
| --- | --- |
| ![Tauri search palette](tauri-palette.jpg) | ![Native GPUI search palette](gpui-palette.jpg) |

## Graph and rich content

Open the fixture and fit the graph. Both frontends show branches, their merge, a linked file, and a user node with an image.
The GPUI graph capture has the original 11 nodes. The Tauri graph capture follows one fixture generation and has 12 nodes.

| Tauri | GPUI |
| --- | --- |
| ![Tauri graph](tauri-graph.jpg) | ![Native GPUI graph](gpui-graph.jpg) |

Open `parity-synthesis`. Both previews show the table, Rust code, formulas, diagram, task list, and unchanged citation text.
The native capture shows inline math on its text line, centered display math, readable table headers, and wide code wrapped within the panel.

| Tauri | GPUI |
| --- | --- |
| ![Tauri rich answer](tauri-rich-answer.jpg) | ![Native GPUI rich answer](gpui-rich-answer.png) |

Expand Provenance and the Read activity, then scroll the panel.
Both captures show indexed references, a partial-evidence warning, a missing citation, and the expanded Read detail.

| Tauri | GPUI |
| --- | --- |
| ![Tauri provenance](tauri-provenance.jpg) | ![Native GPUI provenance](gpui-provenance.jpg) |

## Settings, editor, and files

Open Settings. Inspect provider availability, model selection, global and project preferences, and reasoning effort.
Executable controls continue below the visible portion of the native capture.
The separate manual Browse check selected the offline adapter through the macOS picker and displayed a successful provider validation.

| Tauri | GPUI |
| --- | --- |
| ![Tauri provider settings](tauri-settings.jpg) | ![Native GPUI provider settings](gpui-settings.jpg) |

Open `parity-image` and choose Edit. Both captures show the original source and removable chart thumbnail.
The native editor also shows provider/model controls, Generate, Done, and Attach images.

| Tauri | GPUI |
| --- | --- |
| ![Tauri user editor](tauri-editor.jpg) | ![Native GPUI user editor](gpui-editor.jpg) |

Open the linked Markdown file. Both panels show its name, MIME type, size, Vault-relative path, and text preview.
The native file capture follows the fixture generation, so its toolbar shows 12 nodes and the last-save time.

| Tauri | GPUI |
| --- | --- |
| ![Tauri file preview](tauri-file.jpg) | ![Native GPUI file preview](gpui-file.jpg) |

## Generation and recovery

Generate with the offline fixture. The ACP Turn pauses for a synthetic permission request while its partial response remains visible.
The native capture shows the tool name, request label, Allow once, Deny, and the active generation state.
This native permission capture has 12 nodes. The manual check selected Allow once, then observed completion and autosave.

| Tauri | GPUI |
| --- | --- |
| ![Tauri ACP permission](tauri-permission.jpg) | ![Native GPUI ACP permission](gpui-permission.jpg) |

The additional native capture shows a later fixture Turn with 14 nodes.
The selected image user is visibly dimmer than the independent user on the left, while its white selection border remains visible.
The permission overlay shades both cards uniformly; the additional difference comes from the blocked card's 65% opacity.
Responding remains visible in the toolbar.

![Native GPUI blocked card during generation](gpui-blocked.jpg)

Open Recovery snapshots after changing the isolated fixture. Both captures show saved snapshots with source paths and timestamps.
Native persistence tests separately verify that restoring a snapshot opens an unsaved project and preserves the source file.
The native Settings, recovery, and search captures use the 12-node fixture state.

| Tauri | GPUI |
| --- | --- |
| ![Tauri recovery](tauri-recovery.jpg) | ![Native GPUI recovery](gpui-recovery.jpg) |

## Manual OS checks

These macOS interactions were observed in the running native application on 2026-10-03:

- Open the project picker, then Cancel. The original 11-node fixture remains unchanged.
- In Settings, choose Codex Browse. Use Go to Folder to select `scripts/gpui-fixture-agent.py`, then Choose. Validation reports `codex-acp deterministic fixture 1.0 (offline)`.
- Use toolbar Import and choose `test/fixtures/kagi-export-v1.json`. An unsaved four-node graph opens with the title `Example research conversation`.
- Right-click the canvas, choose Add file, then cancel the macOS picker. The imported graph retains four nodes and gains no attachment.
- Generate from the fixture user, select Allow once, and wait for completion. The toolbar shows 12 nodes and a completed save time.
- Start with a fresh configuration. Choose notes directory, then cancel the OS picker. Welcome remains open. Choose the fixture Vault; the project chooser opens automatically.
- From the project chooser, select Import Kagi. The macOS import picker opens. Cancel preserves the chooser.
- Scroll Settings. Only its dialog content moves; the graph remains stationary. Click the covered New toolbar position. Settings closes without creating a project.
- Choose Jump from the search palette. The matching rich-answer card becomes selected with a white border and centered at 100% zoom.
- During another fixture Turn, compare the selected image user with an independent user. The blocked card is dimmer and retains its selection border.

Native regressions verify that Settings, search, and permission overlays cover the full window, including the toolbar.
Scrolling or clicking these overlays cannot move or clear the graph underneath them.

## Finder drop checks

On 2026-10-04, the user performed both checks in ThoughtTree Native Review with the isolated parity project and confirmed both succeeded.
These checks complete P11 and F02 in the matrix. The images below are the user's supplied screenshots, stored without modification.

1. Drag `fixture-notes.md` from Finder onto empty graph space. A new file card appears with its name, size, and content preview.
2. Drag `vault/synthetic-chart.png` onto a green user card. A new chart thumbnail appears on the “How can the desktop” card.

Both source files are inside `/private/tmp/thoughttree-parity-run/vault`.
The image already present on the separate “An attached image” card belongs to the original fixture.

| File drop onto the graph | Image drop onto a user card |
| --- | --- |
| ![User-confirmed native file drop](gpui-file-drop.png) | ![User-confirmed native image drop](gpui-image-drop.png) |

Native tests separately cover drop coordinate transforms, multiple-file placement, attachment validation, editor routing, thumbnails, and removal controls.
