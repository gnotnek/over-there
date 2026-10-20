# Phase 1: UI refresh

Status: done. Part of the [roadmap](../roadmap.md).

## Why

Components look wonky: buttons and panels are square while inputs are 3px and the page sheet is 16px, the "Find routes" button is 46px next to 44px inputs, and nothing has depth. The owner chose to rebrand around the visual language of the [Pilo autonomous taxi case study](https://www.behance.net/gallery/249780345/Pilo-Autonomous-Taxi-App-UXUI-Branding): black, electric lime and white with soft rounded shapes. The refresh also sets the look that Phase 2 (routes near me) is built in.

## Goals

- Every page uses one palette, one type pair, one radius scale and one height for controls.
- Pages look like the chosen mockup: light app surfaces, black navigation and actions, lime as the highlight, dark map cards.
- Nothing about what pages do changes.

## Non-goals

- New pages, layout changes driven by Phase 2 (the Explore map comes in Phase 2), or a dark mode toggle.
- New JavaScript, CSS frameworks or build steps.

## Anti-goals

- Lime used as text or thin lines on white or light gray; it fails contrast. Lime is a fill behind black text, or a color on black.
- Hover effects that stick on touch screens, or motion that ignores `prefers-reduced-motion`.
- Copying Pilo's logo, car imagery or wording. Only the palette and shape language are borrowed.

## Palette (replaces the five brand colors)

| Token | Value | Use |
|---|---|---|
| black | `#0F0F0F` | text, navigation pills, primary dark buttons, map cards, footer |
| lime | `#C6FF2D` | main call to action fill, active chip, route line on dark, logo mark |
| white | `#FFFFFF` | cards, inputs, text on black |
| charcoal | `#2F2F2F` | raised surfaces on black, map streets, secondary dark chips |
| gray | `#808080` | large secondary text and decorative lines only |

Derived shades, decided by the builder within this family: page background light gray (about `#F2F2F2`), a darker gray for small muted text that passes WCAG AA on white and on the page background (`#808080` does not for body-size text), hairline borders, and a muted red for destructive actions and errors.

## Type

Manjari (700) for display headings and the wordmark, Inter (400–700) for everything else, both from Google Fonts in the existing `<link>`. They replace Libre Franklin and Source Serif 4; text that used the serif becomes Inter.

## Shapes and depth

- Buttons and filter chips are pills. One control height (44px) for buttons, inputs and selects so rows line up; inputs get a 14px radius.
- Cards and panels 20–24px radius, the page sheet and map cards 28px, thumbnails 14px, avatars and icon buttons round.
- Soft layered shadows on white cards and floating controls only; none on flat page sections.
- Hover: cards and primary buttons lift slightly using transform, wrapped in `@media (hover: hover)`; disabled under `prefers-reduced-motion`.
- Focus ring: visible on every control, lime ring with a black inner edge so it shows on both light and dark.

## Page notes

- **Header:** light background, wordmark with a lime mark, navigation as pills with the current page as a black pill, account as a white pill with the username.
- **Home:** the landing page goes dark (black hero, lime highlights, white type), the one place the site follows Pilo's dark marketing pages. Photos keep their places; the teal overlays become black.
- **Explore, dashboard, profile, forms, settings, admin, auth, error:** light gray page, white rounded sheets and cards, black and lime controls.
- **Route page:** map in a 28px rounded card. Route line black with a lime casing so it reads on the light raster tiles; start and finish markers black and lime. Elevation chart line black over a light lime fill.
- **Footer:** black with white text and the lime mark.

## Where it changes

`static/style.css` (rewritten around CSS custom properties for the tokens above), the font `<link>` and `theme-color` in `templates/page.hbs`, the colors in `static/route.js`, and the palette sentence in `docs/architecture.md`. Templates change only where a class is missing.

## Checks

`cargo fmt --check`, `cargo check`, `cargo test` still pass (templates are rendered in web tests). Before and after screenshots of home, explore, route detail, upload form and sign-in at desktop and phone widths for the pull request.

## Left to the builder

- Exact derived shades, shadow values and spacing.
- Whether to restructure `style.css` into sections or keep it compact, as long as tokens drive every color.
- Icon choices for round icon buttons, using text or inline SVG already in the templates.
