# Repository review context

- Desktop UI uses React. Global styles and design tokens live in `desktop/src/app.css`; shared components live in `desktop/src/ui.jsx`.
- Shared components: `Panel`, `Btn`, `Seg`, `Field`, `Check`, `Dot`, `Text`, `Bar`; `I` contains SVG icons.
- Colour tokens:
  - Surfaces: `--bg: #1A1D2A`, `--panel: #23273A`, `--head: #2B3047`, `--rule: #343953`, `--raised: #454B6B`, `--sel: #3A2650`, `--group: #1F2335`.
  - Text: `--text: #E8EAF2`, `--text-2: #C9CDE0`, `--muted: #8F94AD`, `--sec: #A9AEC7`.
  - Actions: `--accent: #F52ACB`, `--on-accent: #14061A`, `--accent-text: #FF6BE0`.
  - Status: `--mint: #3EF2A3`, `--cyan: #37D2E8`, `--coral: #FF5C7A`, `--amber: #FFC24D`, `--violet: #B48BFF`, `--peach: #FF8A5C`.
- Design convention: deep navy panels, magenta actions, mint completion. Tokens are defined in `:root`.
- Typography: body uses 12px/1.4 system sans; `--mono` uses SF Mono, ui-monospace, Menlo, monospace. Panel headings and buttons use 11.5px semibold.
- Layout: app padding and main/column gaps are 10px; panels have 6px corners, 26px header strips, and body padding of 8px 10px. Standard controls are 26px tall with 4px corners. Spacing and sizes are CSS literals, not shared tokens.
- `Btn` supplies hover/disabled styling; `.input:focus` uses the accent border.
- Tauri production configuration is `desktop/src-tauri/tauri.conf.json`; development overrides are in `tauri.dev.conf.json`.
- Bundling targets macOS `app` and `dmg`, minimum macOS 13. Production and development use separate identities and `icons/` versus `icons-dev/`.
- Both icon configurations reference `32x32.png`, `128x128.png`, `128x128@2x.png`, and `icon.icns`.
