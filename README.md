# Over There

Over There is an open source GPX route library for cyclists. Riders can discover routes, inspect them on an interactive map, share their own GPX files, and download routes for their devices. It is built as a small Rust server with server rendered pages and a MapLibre map.

## What works

- Accounts, email verification, profiles and contributor badges managed by an administrator
- A local disposable-email domain blocklist with no lookup fee or user data sent to a checker
- GPX tracks and routes with segment-safe distance, optional elevation and duration
- Public, unlisted and private routes with gated geometry, images and downloads
- Optional hiding of start and finish areas from the public map and visitor GPX
- Route upload, editing, deletion, discovery filters and download counts
- Reporting and moderation through `/admin`

The product scope and stack decision are documented in [docs/architecture.md](docs/architecture.md). Route planning, comments, collections and navigation are intentionally outside the MVP.

## Local development

Install Rust 1.88 or newer, copy `.env.example` to `.env`, then run:

```sh
cargo run
```

Open <http://127.0.0.1:3000>. The app creates `data/` and applies SQLite migrations on startup. When Brevo variables are absent, registration logs a development verification link to the server console. Follow that link before uploading. Set `RUST_LOG=over_there=info` if it is not visible.

To make the first moderator after registration, use the local SQLite database:

```sh
sqlite3 data/over-there.db "UPDATE users SET is_admin=1 WHERE username='your_username';"
```

Run `cargo test` for the domain rules. A public install needs `BASE_URL` set to its HTTPS origin, `APP_ENV=production`, `MAP_STYLE_URL` set to a production tile style, `BREVO_API_KEY`, and `MAIL_FROM`. Brevo's free plan currently includes 300 sends per day; the app caps verification sends at 250/day to reserve headroom. Configure a verified sender in Brevo. No paid email plan is required for an early MVP. Never serve `STORAGE_DIR` directly from a web server. GPX files and optimized images are read through permission-checked handlers.

Before public launch, run `python3 scripts/update_disposable_domains.py` and rebuild. This downloads the [MIT-licensed, DNS-checked domain list](https://github.com/groundcat/disposable-email-domain-list) into `resources/disposable-domains.txt`. Refresh it periodically and review changes in Git. The starter list catches common providers; no blocklist can prove an address is permanent, so email verification and rate limits remain necessary. Users whose legitimate domain is incorrectly listed need a manual review path. The checker runs locally and has no per-signup cost.

## Deployment

Build with `cargo build --release --locked`, or use `docker compose up --build -d`. The Compose service binds only to localhost; place a TLS reverse proxy in front. The Docker volume contains both SQLite and uploaded files. Set the deployment environment in `.env` before starting. Keep the volume persistent across container upgrades.

For backups, take an online SQLite backup (`sqlite3 data/over-there.db '.backup /backup/over-there.db'`) and copy `data/files` in the same maintenance window. Restore both before starting the app. Back up encryption and provider configuration separately. Test a restore regularly. SQLite WAL mode means copying just the main database file from a running service is unsafe.

The included raster OSM style is for local development. Public traffic should use a tile provider whose terms cover the expected traffic and include attribution. Rate limit login, registration, reporting and uploads at the reverse proxy as well as the app's per-account login throttle.

## License

MIT. See [LICENSE](LICENSE).

The two cycling photos in `static/images/` were generated for this project and are included with the source.

To publish the source under your GitHub account, run `bash scripts/publish-github.sh`. It signs in with GitHub CLI if needed, initializes the local Git history, and creates or pushes the public `gnotnek/over-there` repository.
