# UI Refresh Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restyle every page in the black, electric lime and white palette with Manjari and Inter, pill controls and large rounded cards, without changing behaviour.

**Architecture:** All colors, radii, shadows and the control height become CSS custom properties in one `:root` block in `static/style.css`; every rule reads tokens. A Rust test guards that no color literal lives outside `:root` and that token pairs meet WCAG AA. The template only changes its font link and `theme-color`; `static/route.js` gets the new map and chart colors.

**Tech Stack:** Plain CSS, Handlebars (`templates/page.hbs`), vanilla JS (`static/route.js`), Rust tests in `src/presentation/web.rs`.

**Spec:** [docs/specs/2026-10-20-ui-refresh-design.md](../specs/2026-10-20-ui-refresh-design.md)

## Global Constraints

- Palette: black `#0F0F0F`, lime `#C6FF2D`, white `#FFFFFF`, charcoal `#2F2F2F`, gray `#808080`. Remove `#006D77`, `#83C5BE`, `#EDF6F9`, `#FFDDD2`, `#E29578` and every shade derived from them.
- Lime is never text or a thin line on white or light gray; only a fill behind black text, or a color on black.
- `#808080` only for large text and decorative lines; small muted text uses `--muted`.
- Fonts: Manjari 700 for display headings and the wordmark, Inter 400–700 for everything else. Libre Franklin and Source Serif 4 are removed.
- One control height `44px` for buttons, inputs and selects. Inputs radius 14px, cards and panels 20–24px, page sheet and map cards 28px, thumbnails 14px, buttons and chips pills.
- Hover lifts only inside `@media (hover: hover)`; all motion off under `prefers-reduced-motion: reduce`.
- No new JavaScript, CSS frameworks or build steps. No Pilo logo, imagery or wording.
- `cargo fmt --check`, `cargo check`, `cargo test` pass after every task.
- Work on branch `feat/ui-refresh`, never on `main`.

## Review Focus

- Phone width (375px): header nav, filter form, route actions and dashboard table must not overflow horizontally or clip pill text.
- Keyboard focus must be visible on the dark home page and on light pages, including on lime buttons.
- Touch screens: a tapped card or button must not stay lifted after the tap.
- A route without a cover photo shows the generated SVG preview; its contour and line classes must use the new tokens and stay visible on the card.
- Long route titles, long usernames in the header avatar link and empty states (no routes, no reports) must wrap or truncate cleanly.

Each line is checked in the visual step of the task that owns it (Tasks 2–5).

## File map

- `static/style.css`: rewritten in place. Keep the existing section order (base and shell, home, explore and app sheet, route page, profile and dashboard, forms, auth, settings, admin, media queries, map helpers); delete the trailing override blocks by folding them into their sections.
- `templates/page.hbs`: font link line 15, `theme-color` line 6. Class additions only if a task names them.
- `static/route.js`: lines 20–21 (map line layers) and 47 (elevation SVG).
- `src/presentation/web.rs`: new tests in the existing `mod tests`.
- `docs/architecture.md`: the palette sentence in "Primary flows and page structure".

## Visual check procedure (used by Tasks 2–5)

The running server compiles `page.hbs` into the binary, so restart it after template edits; CSS and JS are read from disk on each request. Start a second instance so the owner's server on port 3000 is untouched:

```bash
LISTEN_ADDR=127.0.0.1:3100 BASE_URL=http://127.0.0.1:3100 cargo run
```

Open the pages named by the task at 1280px and 375px width in the built-in browser and screenshot each. Use a synthetic GPX and a test account created through the app; never real routes.

---

### Task 0: Branch and record the design docs

**Files:**
- Modify: `.gitignore` (add `.superpowers/`)
- Add: `docs/roadmap.md`, `docs/specs/`, `docs/plans/`, `CONTRIBUTING.md`

- [ ] **Step 1:** `git switch -c feat/ui-refresh`
- [ ] **Step 2:** Append `.superpowers/` to `.gitignore`; `git status --short` shows no `.superpowers` entry.
- [ ] **Step 3: Commit**

```bash
git add .gitignore CONTRIBUTING.md docs/roadmap.md docs/specs docs/plans
git commit -m "docs: add roadmap, phase designs and contribution path"
```

### Task 1: Tokens, palette swap and guard tests

**Files:**
- Modify: `static/style.css` (whole file, colors only), `templates/page.hbs:6,15`, `static/route.js:20-21,47`, `docs/architecture.md` (palette sentence)
- Test: `src/presentation/web.rs` (`mod tests`)

**Interfaces:**
- Produces, in `:root` of `static/style.css` (later tasks use only these names):
  - Colors: `--black:#0F0F0F` `--lime:#C6FF2D` `--white:#FFFFFF` `--charcoal:#2F2F2F` `--gray:#808080` `--bg:#F2F2F2` `--muted:#5C5C5C` `--line:#E2E2E2` `--lime-wash:#F1FFD0` `--danger:#B42318`
  - Effects: `--overlay` (black at 55%), `--shadow-sm`, `--shadow-md`, `--shadow-lg` (layered, black at 4–14%), `--ring` (focus ring: `0 0 0 2px var(--black), 0 0 0 5px var(--lime)`)
  - Shape: `--control-h:44px` `--r-control:14px` `--r-card:22px` `--r-sheet:28px` `--r-thumb:14px` `--pill:999px`
  - Motion: `--ease:cubic-bezier(.16,1,.3,1)`
  - Type: `--display:"Manjari",system-ui,sans-serif` `--font:"Inter",system-ui,sans-serif`

- [ ] **Step 1: Write the failing tests** in `mod tests`:

`stylesheet_colors_live_only_in_root_tokens`, reading the files with `include_str!`, asserts:
- `style.css` starts with `:root{`;
- after the first `}` there is no `#` followed by a hex digit, and no `rgb(` or `rgba(`;
- `static/route.js` and `templates/page.hbs` contain none of `#006D77`, `#83C5BE`, `#EDF6F9`, `#FFDDD2`, `#E29578` (case-insensitive).

```rust
#[test]
fn palette_token_pairs_meet_wcag_aa() {
    // parse "--name:#RRGGBB" pairs from the :root block of style.css
    let pairs = [
        ("--black", "--bg"), ("--black", "--white"), ("--muted", "--white"),
        ("--muted", "--bg"), ("--black", "--lime"), ("--white", "--black"),
        ("--lime", "--black"), ("--danger", "--white"), ("--white", "--charcoal"),
    ];
    for (fg, bg) in pairs {
        assert!(contrast(token(fg), token(bg)) >= 4.5, "{fg} on {bg}");
    }
}
```

`token(name) -> [u8; 3]` reads the hex value from `:root`; `contrast(a, b) -> f64` is the WCAG 2.x ratio (sRGB channel `c/255`, linearise `c<=0.04045 ? c/12.92 : ((c+0.055)/1.055)^2.4`, `L=0.2126R+0.7152G+0.0722B`, ratio `(L1+0.05)/(L2+0.05)` with `L1` the lighter). Both are private helpers in the test module.

- [ ] **Step 2:** `cargo test stylesheet_colors palette_token` → both FAIL (no `:root` tokens yet; old hexes present).
- [ ] **Step 3: Swap the palette.** In `static/style.css` write the `:root` block from Interfaces as the first rule, then replace every color literal in the file with a token, mapping by role: old teal fills and dark text → `--black`; `#EDF6F9` page/panel backgrounds → `--bg`; `#C7E5E6`/`#83C5BE` borders → `--line`; muted text → `--muted`; peach/coral highlights and the focus outline → `--lime` (as fill) or `--ring`; teal photo overlays → `--overlay`; `#8C3B25` → `--danger`; `white` → `--white`. Replace `--font`/`--serif` usages with `--font` and headings with `--display`. Do not change radii or layout in this task.
- [ ] **Step 4:** `templates/page.hbs`: `theme-color` → `#0F0F0F`; font link → `https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=Manjari:wght@700&display=swap`.
- [ ] **Step 5:** `static/route.js`: `route-casing` color `#C6FF2D`, `route-line` color `#0F0F0F`; elevation fill `#F1FFD0`, stroke `#0F0F0F`, cursor `#2F2F2F`.
- [ ] **Step 6:** `docs/architecture.md`: replace the five-color sentence with the Pilo palette and the lime-as-fill rule, linking the spec.
- [ ] **Step 7:** `cargo fmt --check && cargo check && cargo test` → all PASS.
- [ ] **Step 8: Commit** `feat(ui): move styles to tokens with black and lime palette`

### Task 2: Shell and shared components

**Files:** Modify `static/style.css` (base and shell section, buttons, inputs, app frame and sheet, page heading, filter form, route cards, empty state, pagination, notice, map helpers, both media queries for these selectors).

**Interfaces:** Consumes Task 1 tokens only.

- [ ] **Step 1: Header and footer.** Header background `--bg`, text `--black`, no bottom border; brand wordmark in `--display`; brand mark stroke `--black` on a `--lime` rounded square (`--r-thumb`). Nav links are pills (`--pill`, padding 9px 16px); `[aria-current]` is a `--black` pill with `--white` text; hover is `--white` fill. `.header-join` is a `--lime` pill with `--black` text. `.account-link` is a `--white` pill with `--shadow-sm`, truncating long usernames with ellipsis at 16ch. Footer `--black`, text `--white`, brand in `--lime`.
- [ ] **Step 2: Controls.** `.button`: `height:var(--control-h)` (replace `min-height:46px`), `border-radius:var(--pill)`, `--black` fill, `--white` text, padding 0 22px. `.button-light`: `--lime` fill, `--black` text. `.button-outline`: `--white` fill, `--black` text, `1px solid var(--line)`. Inputs, selects, textareas: `height:var(--control-h)` (textarea `min-height`), `--r-control`, `1px solid var(--line)`, `--white`. `:focus-visible{outline:none;box-shadow:var(--ring)}` for every interactive element. Delete `.filter-form .button{height:44px}`.
- [ ] **Step 3: Surfaces.** `.app-frame` `--bg`; `.app-sheet` `--white`, `--r-sheet`, no border, `--shadow-sm`. `.filter-form` `--bg`, `--r-card`. `.route-card` `--white`, `--r-card`, `overflow:hidden`, `--shadow-sm`; `.route-preview` `--r-thumb` inset 8px; preview SVG `.contour` stroke `--line`, `.route-line` stroke `--black`. `.empty-state` `--bg`, `--r-card`. Explore `.page-heading` photo `--r-card`, overlay `--overlay`. `.notice` `--black` pill with `--shadow-lg`. Map markers: `.map-point` `--lime` with `--black` border; `.map-finish` reversed; `.map-hover` `--white` with `--black` border.
- [ ] **Step 4: Motion.** Wrap card and button hover lifts (`translateY(-2px)` buttons, `-4px` cards, shadow to `--shadow-md`) in `@media (hover:hover)`, transitions `.25s var(--ease)`. Extend the reduced-motion block to set `transition:none` on `.button` and `.route-card` and remove transforms.
- [ ] **Step 5: Visual check** (procedure above): explore with routes, explore with no results, header signed in and signed out, at 1280px and 375px. Pass when: filter inputs and Find routes share one height and baseline; Review Focus lines 1, 2, 3 and 5 hold; a coverless route shows a visible black preview line.
- [ ] **Step 6:** `cargo test` PASS. **Commit** `feat(ui): pill controls, rounded surfaces and soft depth`

### Task 3: Dark home page

**Files:** Modify `static/style.css` (home section and its media-query rules).

- [ ] **Step 1:** `.home-body` and `.hero` `--black` with `--white` text; the header on the home page (`.home-body .site-header`) also `--black` with white links and the `[aria-current]`/hover pill as `--charcoal`. Hero `h1` in `--display`; a lime highlight is allowed only as a fill (e.g. `.button-light`).
- [ ] **Step 2:** `.route-sheet` becomes a `--charcoal` card, `--r-sheet`, no rotation clip-path artefacts, photo `--r-card`. `.home-latest` stays light (`--bg`) so cards read as on other pages. `.home-process` `--black` with the photo `--r-sheet`; `.process-slip` (if still present) `--charcoal`. `.empty-home` `--lime` fill with `--black` text.
- [ ] **Step 3: Visual check:** `/` with and without routes, 1280px and 375px. Pass when: focus ring visible on the dark hero buttons (Review Focus 2); no lime text on light sections; no horizontal scroll at 375px.
- [ ] **Step 4:** `cargo test` PASS. **Commit** `feat(ui): dark home page in the new palette`

### Task 4: Route page

**Files:** Modify `static/style.css` (route page section and media rules). `static/route.js` colors were set in Task 1.

- [ ] **Step 1:** `.route-layout` gap 12px on `--bg`; `.route-main` and `.route-side` `--white`, `--r-sheet`. `.route-map` `--r-sheet` with `overflow:hidden` inside an 8px inset. `.metric-strip` numbers in `--display`; `.verified-badge` `--lime` pill with `--black` text; `.privacy-note` `--bg`, `--r-control`; `#elevation-chart` `--bg`, `--r-card`; `.photo-grid img` `--r-thumb`; avatar letters `--charcoal` with `--white` text.
- [ ] **Step 2: Visual check:** a public route with elevation and photos, and one with endpoint hiding, 1280px and 375px. Pass when: the route line reads as black with a lime edge on the light tiles; elevation hover cursor visible; Download and Share buttons line up; long titles wrap (Review Focus 5).
- [ ] **Step 3:** `cargo test` PASS. **Commit** `feat(ui): route page cards, map and chart colors`

### Task 5: Forms, auth, dashboard, profile, settings, admin, error

**Files:** Modify `static/style.css` (profile/dashboard, forms, auth, settings, admin, error sections and media rules).

- [ ] **Step 1:** `.form-aside` `--bg`, `--r-card`. `.danger-form` top border `--line`, button text `--danger`. `.auth-shell` `--black`; `.auth-intro` photo overlay `--overlay`, no coral top border; `.auth-form` `--white` panel. `.dashboard-stats` numbers `--display`; `.table-row` hover `--bg` with `--r-control`. `.admin-row button` becomes a small pill (`height:36px`, `--line` border). `.error-page` `--bg`.
- [ ] **Step 2: Visual check:** upload (verified and unverified), edit with delete, register, login, account, dashboard with and without routes, profile, admin with and without reports, a 404, at 1280px and 375px. Pass when Review Focus 1 and 5 hold on every page and every control is 44px tall except admin pills.
- [ ] **Step 3:** `cargo fmt --check && cargo check && cargo test` PASS. **Commit** `feat(ui): forms, account and admin pages in the new style`

### Task 6: Final check

- [ ] **Step 1:** `grep -niE '#006D77|#83C5BE|#EDF6F9|#FFDDD2|#E29578|Libre Franklin|Source Serif' static templates src docs/architecture.md` → no matches.
- [ ] **Step 2:** Screenshots of home, explore, route detail, upload and sign-in at 1280px and 375px saved outside the repo for the pull request description (before shots come from `main` on port 3000).
- [ ] **Step 3:** Mark Phase 1 done in `docs/roadmap.md`; commit `docs: mark UI refresh done`.
