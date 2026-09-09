# ThoughtTree design

One system for the desktop app and the landing page. Decisions from the 2026-09 design interview, revised after the icon was chosen. Tokens live in `tokens.css`. Icon source is `logo.svg`, reduced mark is `logo-mark.svg`.

## Intent

ThoughtTree is a quiet tool for researchers and knowledge workers. The graph is the product. Chrome stays out of the way. One colour, used for meaning.

Reference: the app icon. Mint tree on teal-black. Filled root, outlined children, one merge. Everything else follows from it.

## Decisions

| Area | Decision |
|---|---|
| Theme | Dark only. No light theme, no toggle. |
| Palette | Teal-black neutral scale. One mint accent. One red for danger. |
| Mint `#c0facc` | Nodes, links, primary action, focus, selection. |
| Red `#f0857f` | Errors, destructive confirms, stale-save warnings. Only red. |
| Retired | Yellow `#F5B82E`, cyan `#29C2D6`, figure-eight logo, blue, green, Catppuccin greys. |
| Role encoding | Fill, not hue. User = filled mint. Assistant = mint outline. Same as the icon. |
| Type | System sans everywhere. Mono only for code, paths, kbd. |
| Surfaces | Flat. 1px hairline borders. 6px radius. No shadows, gradients, blur, gloss. |
| Canvas | Plain `--tt-bg`. No grid. |
| Nodes | Fixed 170×120px landscape rectangles (1:√2, like DIN A). Role shown by a filled or outlined dot in the corner, plus fill weight of the border. See below. |
| Motion | 120 to 180ms colour and opacity fades. No spatial animation. Landing hero draws edges once. Reduced-motion drops everything. |
| Audience | Researchers first. Readable over dense. |

## Colour rules

1. Teal-black carries structure. Mint carries meaning. Red carries danger.
2. Mint never appears as decoration. If a mint element is not a node, a link, a primary action, focus, or selection, make it muted text.
3. Fill means user. Outline means assistant. Never encode role with a second hue.
4. `--tt-accent-soft` (12% alpha) is the only tinted background. Used behind selected rows and role badges.
5. Text on mint is `--tt-on-accent`. Never white on mint.
6. Links are mint, no underline at rest, underline on hover.
7. Contrast floor is WCAG AA. Every token pair in `tokens.css` passes on `--tt-surface`. `--tt-edge` is decorative and exempt.

## Type scale

Six sizes, no others. Body 15px at 1.6. Headings use `--tt-leading-tight`. Weight 400 body, 600 headings and buttons. 700 only for the landing h1.

Paragraph measure caps at 65ch on landing and in the side panel.

## Components

### Buttons
- Primary: `--tt-accent` fill, `--tt-on-accent` text. One per view.
- Secondary: transparent, `--tt-line` border, `--tt-text`.
- Ghost: no border, `--tt-text-muted`. Toolbars.
- Destructive: secondary style with `--tt-danger` text and border.
- Hover changes colour only.

### Nodes
- 170×120px landscape (1:√2), `--tt-surface`, `--tt-line` border, 6px radius.
- Role dot, 10px, top-left inside padding: user = filled mint, assistant = 2px mint outline. Sticky notes have no dot.
- Selected: border `--tt-accent`. Nothing else changes.
- Streaming: role dot pulses opacity 0.4 to 1 over 1.2s. Only animation on the canvas.
- Blocked (ancestor streaming): opacity 0.5.
- Error: border `--tt-danger`, role dot becomes red.
- Preview text: `--tt-size-sm`, `--tt-text-muted`, 4 lines max, then fade.

### Edges
- `--tt-edge`, 1.5px, no arrowheads.
- Selected: `--tt-edge-selected` (mint).
- Multi-parent edges look like any other. The merge node's two incoming edges do the signalling.

### Side panel
- `--tt-surface`, `--tt-line` left border, no shadow.
- Role badge at top: role dot plus label, `--tt-size-xs`, `--tt-text-muted`. No tinted pill.
- Markdown body at `--tt-size-md`. Code blocks on `--tt-surface-2`.

### Toolbar
- `--tt-surface`, bottom hairline. Ghost buttons. Icons 16px, 1.5px stroke, `--tt-text-muted`, `--tt-text` on hover.

### Palette (Cmd+K)
- `--tt-surface`, hairline, 6px radius. Selected row `--tt-accent-soft`. Matched text `--tt-text`, rest `--tt-text-muted`.

### Dialogs
- `--tt-surface`, hairline, 6px radius, 24px padding. Backdrop `rgba(4,37,44,.75)`, no blur. Title `--tt-size-xl`.

### Inputs
- `--tt-surface-2`, `--tt-line` border, focus border `--tt-accent`, focus ring `--tt-focus`.

### Kbd
- `--tt-mono`, `--tt-size-xs`, `--tt-surface-2`, hairline, 4px radius, 1px 6px padding.

## Landing page

Same tokens, same rules. Differences allowed:
- Hero h1 at `--tt-size-hero`, weight 700, no coloured word.
- Section padding `--tt-s8`.
- Example graphs use the icon grammar: filled mint root, outlined mint nodes, `--tt-edge` edges 1.5px. No second colour.
- No cards. Features and steps are lists with hairlines.
- Nav uses `logo-mark.svg` at 24px in `currentColor`.

## Iconography

- App icon: `logo.svg`. Mint tree on teal-black squircle. Filled root, ten outlined nodes, one merge. Ship at 1024 and let macOS scale.
- Reduced mark: `logo-mark.svg`. Four nodes, one merge, `currentColor`. Use at 16 to 32px: nav, favicon, menu bar, about dialog. Never scale the full icon below 48px.
- Wordmark: "ThoughtTree", system sans 600, `--tt-text`. No colour, no tracking.
- UI icons: line style, 1.5px stroke, 16px, `currentColor`. No filled icons, no emoji.

## Copy tone

Short declaratives. No hype, no exclamation marks, no "powerful", no "seamless". Say what happens.

## Migration notes for the app

Current app CSS (checked 2026-09):
- No CSS variables. Hex values inline, ~25 distinct.
- Blue `#3b82f6`/`#60a5fa` on user nodes, buttons, badges. Green `#22c55e`/`#4ade80` on assistant nodes and badges. Both become mint; role moves to fill vs outline.
- Backgrounds `#0f0f17`, `#1a1a2e`, `#0d0d1a` map to `--tt-bg`, `--tt-surface`, `--tt-bg`.
- Text `#e0e0e0`, `#cdd6f4` map to `--tt-text`. `#a6adc8`, `#bac2de` map to `--tt-text-muted`.
- Radii 3/4/6/8/9/12px collapse to 6px, 4px for kbd.
- Node border colour by role becomes the role dot.
- Replace `src-tauri/icons/*` from `logo.svg`.

Suggested order: import `tokens.css` in `App.css`, replace hex values file by file starting with `Graph/styles.css`, then `SidePanel`, `Toolbar`, `Palette`, `SettingsDialog`. Keep the diff mechanical.
