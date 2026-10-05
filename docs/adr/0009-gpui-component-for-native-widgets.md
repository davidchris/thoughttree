# ADR 0009: gpui-component for native widgets

- Status: Accepted
- Date: 2026-10-05

## Context

The native desktop frontend (`crates/thoughttree-gpui`) is built on GPUI, Zed
Industries' UI framework. GPUI supplies windows, layout, text shaping, input
events, and rendering, but no widgets. ThoughtTree needs several widgets that
are expensive to build correctly:

- text inputs with IME composition, multi-line auto-grow, and placeholders for
  the composer, search palette, and project filter;
- a Markdown viewer with selection, tables, task lists, tree-sitter syntax
  highlighting, and extension points for formulas;
- buttons, dropdown menus, icons, a dialog-hosting window root, and theming.

Zed's own UI, editor, and Markdown code lives in the Zed repository and is not
published as standalone crates. gpui-component, Longbridge's component library
for GPUI, provides all of these widgets. Its repository is now named gpui-kit,
and since 0.7 its text and input code lives in the `gpui-base` crate.

The frontend adopted gpui-component when it was first added, but that choice
was not recorded. This ADR records it after the upgrade from 0.5.1 to 0.7.0.

## Decision

The native frontend uses gpui-component for widgets and rich text. The
`gpui-pre`, `gpui-pre-platform`, `gpui-component`, and `gpui-kit-assets`
versions are pinned exactly, and they are upgraded together as one change.

Application-specific rendering is added through gpui-component's extension
points, not through a fork. TeX formulas are `MarkdownPlugin`s that render RaTeX
output as `InlineElement`s (inline math) and custom blocks (display math).

When gpui-component has a defect, we file it upstream with a reproduction and
keep any local workaround in our own code, linked to the issue so it can be
removed. The first such workaround restores display formulas at selection
endpoints (longbridge/gpui-kit#3368).

## Consequences

- ThoughtTree does not maintain its own text input, Markdown viewer, or menus.
- gpui-component is young and changes quickly. The 0.5.1 to 0.7.0 upgrade
  split the crates, renamed the project, replaced multi-line inputs with a
  separate `Textarea`, and added a semantic theme token layer. Upgrades need
  their own review, tests, and visual checks against the parity captures.
- Theme edits must go through `Theme::update`. Edits through `global_mut` do not
  reach the token layer that 0.7 renders with.
- Behavior that gpui-component does not support, such as copying custom
  Markdown blocks at selection endpoints, needs a local workaround until it is
  fixed upstream.
- Vendoring or forking gpui-component remains possible if an upstream gap
  blocks a required feature, but it is not the default.
