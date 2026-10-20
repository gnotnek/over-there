# Repository Guidelines

## Project Structure & Module Organization

This is a Rust 2021 Axum application with server-rendered Handlebars pages. Keep route validation, GPX calculations, and visibility rules in `src/domain/`; account and email rules in `src/identity/`; SQLx route persistence in `src/infrastructure/`; and HTTP handlers in `src/presentation/`. The SQLite schema lives in `migrations/`. The page template is `templates/page.hbs`, with CSS, JavaScript, map style, and generated images in `static/`. Domain and web tests live beside the code in `#[cfg(test)]` modules. Read `docs/architecture.md` before changing access rules or module boundaries.

## Build, Test, and Development Commands

- `cp .env.example .env && cargo run` starts the local server at `http://127.0.0.1:3000` and applies migrations.
- `cargo fmt --check` checks Rust formatting; run `cargo fmt` to fix it.
- `cargo check` checks that the application compiles without producing a release binary.
- `cargo test` runs domain and HTTP tests.
- `cargo build --release --locked` builds a deployable binary using the committed lockfile.

## Coding Style & Naming Conventions

Use rustfmt defaults: four-space indentation, `snake_case` for modules/functions, and `UpperCamelCase` for types. Keep business decisions out of handlers where they can be expressed as domain rules. Follow nearby `Result` and error-handling patterns; avoid adding abstractions for a single caller. Keep browser JavaScript small and limited to interaction such as maps and elevation charts. Preserve the palette documented in `docs/architecture.md` when editing the UI.

## Testing Guidelines

Name tests for behavior, such as `rejects_dtd_and_bad_coordinates`. Add focused tests when changing GPX metrics, route visibility, endpoint hiding, or authentication. Use synthetic GPX and user data; never commit real uploaded routes or private coordinates. Run the formatting, check, and test commands above before a pull request. There is no numeric coverage threshold.

## Commit & Pull Request Guidelines

Use a short, imperative Conventional Commit subject, for example `feat(ui): add cycling imagery and refresh palette`. In pull requests, describe user impact, link a relevant issue, list checks run, and call out schema or configuration changes. Include before/after screenshots for visible UI changes. Open an issue before a large scope change; small fixes can go straight to a pull request.

## Security & Configuration

Never commit `.env`, `data/`, credentials, or uploaded GPX. Keep private files behind authorization checks; public maps, downloads, and elevation data must honor endpoint hiding. Report vulnerabilities privately as described in `SECURITY.md`.
