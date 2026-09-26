# Desktop design system

- `desktop/src/app.css` defines global styles and `:root` tokens.
- Surface tokens: `--bg`, `--panel`, `--head`, `--rule`, `--raised`, `--sel`, `--group`.
- Text tokens: `--text`, `--text-2`, `--muted`, `--sec`.
- Action tokens: `--accent`, `--on-accent`, `--accent-text`. Status colours: `--mint`, `--cyan`, `--coral`, `--amber`, `--violet`, `--peach`.
- Body typography is 12px/1.4 system sans-serif. `--mono` supplies the monospace stack.
- `app.css` loads `fonts/GreatVibes.ttf` as “Great Vibes”, normal weight 400. `.brand` uses it at 24px with a cursive fallback.
- Layout uses 10px outer padding and panel gaps, 6px panel radii, 26px panel headers, and 8px 10px panel-body padding.
- `desktop/src/ui.jsx` provides shared `Panel`, `Btn`, `Seg`, `Field`, `Check`, `Brand`, `FileMark`, and `Confirm` components.
- Standard buttons and inputs are 26px high with 4px radii. Input focus uses `--accent`; disabled buttons use reduced opacity.
- `Brand` supports platform colours or `currentColor` through its `mono` prop.
