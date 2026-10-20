use crate::{
    domain::{
        gpx, nearby,
        route::{self, RouteDraft, Visibility},
    },
    identity::{
        auth::{self, User},
        email,
    },
    infrastructure::routes::{self, RouteRecord},
};
use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Form, Multipart, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Router,
};
use handlebars::Handlebars;
use image::ImageFormat;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    SqlitePool,
};
use std::{
    collections::HashMap,
    env,
    path::{Path as FilePath, PathBuf},
    str::FromStr,
    sync::Arc,
};
use tower_http::{
    services::ServeDir,
    trace::{DefaultOnResponse, TraceLayer},
    LatencyUnit,
};
use tracing::Level;
use uuid::Uuid;

type WebResult = Result<Response, AppError>;

struct AppState {
    pool: SqlitePool,
    templates: Handlebars<'static>,
    storage: PathBuf,
    base_url: String,
    map_style: String,
    secure_cookie: bool,
    brevo_api_key: Option<String>,
    mail_from: Option<String>,
}
type Shared = Arc<AppState>;

#[derive(Debug)]
struct AppError(StatusCode, String);
impl AppError {
    fn bad(msg: impl Into<String>) -> Self {
        Self(StatusCode::BAD_REQUEST, msg.into())
    }
    fn forbidden() -> Self {
        Self(StatusCode::FORBIDDEN, "This action is not available".into())
    }
    fn not_found() -> Self {
        Self(StatusCode::NOT_FOUND, "Route or page not found".into())
    }
    fn internal(error: impl std::fmt::Display) -> Self {
        tracing::error!(%error, "internal error");
        Self(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Something went wrong".into(),
        )
    }
}
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if self.0.is_client_error() {
            // Messages are fixed user-facing text, so they carry no personal data.
            tracing::info!(status = self.0.as_u16(), reason = %self.1, "request rejected");
        }
        let escaped = escape(&self.1);
        (self.0, Html(format!("<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>Error · Over There</title><link rel=\"stylesheet\" href=\"/static/style.css\"><main class=\"error-page\"><a href=\"/\">Over There</a><h1>{escaped}</h1><p><a href=\"/explore\">Explore routes</a></p></main></html>"))).into_response()
    }
}

struct Viewer {
    user: Option<User>,
    csrf: String,
    set_cookie: Option<String>,
}
async fn viewer(state: &AppState, headers: &HeaderMap) -> Viewer {
    if let Some((user, csrf)) = auth::session(&state.pool, headers).await {
        return Viewer {
            user: Some(user),
            csrf,
            set_cookie: None,
        };
    }
    if let Some(token) = auth::cookie(headers, "ot_csrf")
        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        Viewer {
            user: None,
            csrf: token,
            set_cookie: None,
        }
    } else {
        let csrf = auth::random_token();
        let set_cookie = Some(auth::csrf_cookie(&csrf, state.secure_cookie));
        Viewer {
            user: None,
            csrf,
            set_cookie,
        }
    }
}
fn require_user(v: &Viewer) -> Result<&User, AppError> {
    v.user.as_ref().ok_or_else(auth_required)
}
fn auth_required() -> AppError {
    AppError(
        StatusCode::UNAUTHORIZED,
        "Please sign in to continue".into(),
    )
}
fn csrf(
    headers: &HeaderMap,
    form: &HashMap<String, String>,
    viewer: &Viewer,
    state: &AppState,
) -> Result<(), AppError> {
    if auth::check_csrf(headers, val(form, "csrf"), &viewer.csrf, &state.base_url) {
        Ok(())
    } else {
        tracing::warn!("csrf check failed");
        Err(AppError::forbidden())
    }
}
fn val<'a>(form: &'a HashMap<String, String>, name: &str) -> &'a str {
    form.get(name).map(String::as_str).unwrap_or("")
}
/// Routes an account without a verified email may publish per rolling day.
const UNVERIFIED_DAILY_ROUTES: i64 = 5;
fn daily_limit_reached() -> AppError {
    AppError(
        StatusCode::TOO_MANY_REQUESTS,
        "Unverified accounts can share 5 routes a day. Verify your email to remove the limit"
            .into(),
    )
}
/// Rejects forms whose hidden `website` field was filled, which people never see but bots do.
fn honeypot(form: &HashMap<String, String>) -> Result<(), AppError> {
    if val(form, "website").is_empty() {
        return Ok(());
    }
    tracing::warn!("honeypot field filled, form rejected");
    Err(AppError::bad("Unable to submit this form"))
}
fn escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn base_context(state: &AppState, viewer: &Viewer, title: &str, page: &str) -> Value {
    let user = viewer.user.as_ref().map(|u| json!({"username":u.username,"admin":u.is_admin!=0,"verified":u.email_verified_at.is_some()}));
    let mut data = json!({"title":title,"page":page,"csrf":viewer.csrf,"user":user,"base_url":state.base_url,"map_style":state.map_style});
    data[format!("page_{page}")] = json!(true);
    data
}
fn render(state: &AppState, viewer: Viewer, data: Value) -> WebResult {
    let html = state
        .templates
        .render("page", &data)
        .map_err(AppError::internal)?;
    let mut response = Html(html).into_response();
    if let Some(cookie) = viewer.set_cookie {
        response.headers_mut().append(
            header::SET_COOKIE,
            HeaderValue::from_str(&cookie).map_err(AppError::internal)?,
        );
    }
    Ok(response)
}
fn redirect(path: &str) -> Response {
    Redirect::to(path).into_response()
}
fn msg_redirect(path: &str, message: &str) -> Response {
    let q = message
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect::<String>();
    redirect(&format!("{path}?notice={q}"))
}
fn route_json(r: &RouteRecord) -> Value {
    json!({
        "id":r.id,"slug":r.slug,"title":r.title,"description":r.description,
        "activity":r.activity,"cycling_type":r.cycling_type,"region":r.region,"country":r.country,
        "difficulty":r.difficulty,"tags":r.tags,"visibility":r.visibility,
        "distance_km":format!("{:.1}",r.distance_m/1000.0),
        "gain":r.elevation_gain_m.map(|v|format!("{:.0}",v)),
        "loss":r.elevation_loss_m.map(|v|format!("{:.0}",v)),
        "duration":r.duration_s.map(format_duration),"downloads":r.download_count,
        "cover":r.cover_key.as_ref().map(|_| format!("/routes/{}/cover",r.slug)),
        "preview": preview_svg(&r.geometry_json), "created":r.created_at.get(..10).unwrap_or(&r.created_at),
        "endpoint_hidden":r.hide_endpoints_m>0.0,
    })
}
/// Map pins for listed routes, at the first point of their public geometry.
fn pins_json<'a>(routes: impl Iterator<Item = &'a RouteRecord>) -> String {
    let pins: Vec<Value> = routes
        .filter_map(|r| {
            let [lon, lat] = nearby::first_point(&r.geometry_json)?;
            Some(json!({"slug":r.slug,"title":r.title,"lon":lon,"lat":lat}))
        })
        .collect();
    json!(pins).to_string()
}
fn format_duration(s: i64) -> String {
    format!("{}h {}m", s / 3600, (s % 3600) / 60)
}
fn preview_svg(geometry: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(geometry) else {
        return String::new();
    };
    let Some(lines) = value["coordinates"].as_array() else {
        return String::new();
    };
    let points: Vec<(f64, f64)> = lines
        .iter()
        .flat_map(|l| l.as_array().into_iter().flatten())
        .filter_map(|p| Some((p.get(0)?.as_f64()?, p.get(1)?.as_f64()?)))
        .collect();
    if points.is_empty() {
        return String::new();
    }
    let (mut minx, mut maxx, mut miny, mut maxy) = (180.0f64, -180.0f64, 90.0f64, -90.0f64);
    for &(x, y) in &points {
        minx = minx.min(x);
        maxx = maxx.max(x);
        miny = miny.min(y);
        maxy = maxy.max(y);
    }
    let sx = 340.0 / (maxx - minx).max(0.0001);
    let sy = 190.0 / (maxy - miny).max(0.0001);
    let scale = sx.min(sy);
    let ox = (400.0 - (maxx - minx) * scale) / 2.0;
    let oy = (250.0 - (maxy - miny) * scale) / 2.0;
    let paths = lines
        .iter()
        .filter_map(Value::as_array)
        .map(|line| {
            let d = line
                .iter()
                .filter_map(|p| Some((p.get(0)?.as_f64()?, p.get(1)?.as_f64()?)))
                .enumerate()
                .map(|(i, (x, y))| {
                    format!(
                        "{} {:.1} {:.1}",
                        if i == 0 { "M" } else { "L" },
                        ox + (x - minx) * scale,
                        250.0 - oy - (y - miny) * scale
                    )
                })
                .collect::<Vec<_>>()
                .join(" ");
            format!("<path d=\"{d}\"/>")
        })
        .collect::<String>();
    format!("<svg viewBox=\"0 0 400 250\" role=\"img\" aria-label=\"Route trace preview\"><path class=\"contour\" d=\"M-10 206 Q90 120 175 209 T410 180 M-10 145 Q90 60 175 146 T410 105 M-10 75 Q90 -10 175 76 T410 35\"/><g class=\"route-line\">{paths}</g></svg>")
}

async fn get_route(pool: &SqlitePool, slug: &str) -> Result<RouteRecord, AppError> {
    routes::find_by_slug(pool, slug)
        .await
        .map_err(AppError::internal)?
        .ok_or_else(AppError::not_found)
}
fn can_view(r: &RouteRecord, user: Option<&User>) -> bool {
    route::may_view(
        Visibility::parse(&r.visibility).unwrap_or(Visibility::Private),
        &r.author_id,
        user.map(|u| u.id.as_str()),
        user.is_some_and(|u| u.is_admin != 0),
    )
}
fn can_edit(r: &RouteRecord, user: &User) -> bool {
    route::may_edit(&r.author_id, &user.id, user.is_admin != 0)
}
fn valid_choice(value: &str, choices: &[&str]) -> bool {
    choices.contains(&value)
}

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "over_there=info,tower_http=info".into()),
        )
        .init();
    let db_url = env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://data/over-there.db".into());
    let storage = PathBuf::from(env::var("STORAGE_DIR").unwrap_or_else(|_| "data/files".into()));
    tokio::fs::create_dir_all(&storage).await?;
    if let Some(parent) = FilePath::new(db_url.trim_start_matches("sqlite://")).parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let opts = SqliteConnectOptions::from_str(&db_url)?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    let updated = routes::backfill_route_boxes(&pool).await?;
    if updated > 0 {
        tracing::info!(updated, "route boxes backfilled");
    }
    let stripped = strip_redacted_times(&pool, &storage).await?;
    if stripped > 0 {
        tracing::info!(stripped, "times removed from redacted GPX files");
    }
    tokio::spawn(purge_every_hour(pool.clone()));
    let mut templates = Handlebars::new();
    handlebars::handlebars_helper!(eq: |a: str, b: str| a == b);
    templates.register_helper("eq", Box::new(eq));
    templates.register_template_string("page", include_str!("../../templates/page.hbs"))?;
    let base_url = env::var("BASE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:3000".into())
        .trim_end_matches('/')
        .to_owned();
    let map_style = env::var("MAP_STYLE_URL").unwrap_or_else(|_| "/static/map-style.json".into());
    let brevo_api_key = env::var("BREVO_API_KEY").ok();
    let mail_from = env::var("MAIL_FROM").ok();
    if env::var("APP_ENV").as_deref() == Ok("production") {
        if !base_url.starts_with("https://")
            || brevo_api_key.is_none()
            || mail_from.is_none()
            || map_style == "/static/map-style.json"
        {
            return Err(
                "Production requires HTTPS BASE_URL, Brevo email configuration, and MAP_STYLE_URL"
                    .into(),
            );
        }
    }
    tracing::info!(
        env = env::var("APP_ENV").as_deref().unwrap_or("development"),
        %base_url,
        storage = %storage.display(),
        email = if brevo_api_key.is_some() { "brevo" } else { "log" },
        custom_map_style = map_style != "/static/map-style.json",
        "configuration loaded"
    );
    let state = Arc::new(AppState {
        pool,
        templates,
        storage,
        secure_cookie: base_url.starts_with("https://"),
        base_url,
        map_style,
        brevo_api_key,
        mail_from,
    });
    let app = router(state);
    let addr = env::var("LISTEN_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".into());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(%addr,"Over There listening");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn purge_every_hour(pool: SqlitePool) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(3600));
    loop {
        tick.tick().await;
        match auth::purge_expired(&pool).await {
            Ok(p) if p != auth::Purged::default() => tracing::info!(
                sessions = p.sessions,
                verifications = p.verifications,
                attempts = p.attempts,
                "expired records purged"
            ),
            Ok(_) => {}
            Err(error) => tracing::error!(%error, "purging expired records failed"),
        }
    }
}

/// Rewrites redacted GPX files written before times were dropped from them. Idempotent.
async fn strip_redacted_times(pool: &SqlitePool, storage: &FilePath) -> Result<u64, sqlx::Error> {
    let keys: Vec<String> =
        sqlx::query_scalar("SELECT public_gpx_key FROM routes WHERE public_gpx_key IS NOT NULL")
            .fetch_all(pool)
            .await?;
    let mut stripped = 0;
    for key in keys {
        let path = storage.join(&key);
        let bytes = match tokio::fs::read(&path).await {
            Ok(bytes) => bytes,
            Err(error) => {
                tracing::warn!(%key, %error, "redacted GPX unreadable");
                continue;
            }
        };
        if !bytes.windows(6).any(|w| w == b"<time>") {
            continue;
        }
        let segments = match gpx::parse(&bytes) {
            Ok(parsed) => parsed.segments,
            Err(error) => {
                tracing::warn!(%key, %error, "redacted GPX unparsable");
                continue;
            }
        };
        match tokio::fs::write(&path, gpx::redacted_gpx(&segments)).await {
            Ok(()) => stripped += 1,
            Err(error) => tracing::warn!(%key, %error, "redacted GPX not rewritten"),
        }
    }
    Ok(stripped)
}

/// Best-effort removal of stored files whose rows are already gone.
async fn remove_assets(state: &AppState, keys: impl IntoIterator<Item = String>) {
    for key in keys {
        if let Err(error) = tokio::fs::remove_file(state.storage.join(&key)).await {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(%key, %error, "stored file not removed");
            }
        }
    }
}

fn router(state: Shared) -> Router {
    Router::new()
        .route("/", get(home))
        .route("/explore", get(explore))
        .route("/register", get(register_form).post(register))
        .route("/login", get(login_form).post(login))
        .route("/logout", post(logout))
        .route("/verify/{token}", get(verify_email))
        .route("/upload", get(upload_form).post(upload))
        .route("/routes/{slug}", get(route_detail))
        .route("/routes/{slug}/geometry", get(route_geometry))
        .route("/routes/{slug}/download", get(download))
        .route("/routes/{slug}/cover", get(route_cover))
        .route("/routes/{slug}/images/{id}", get(route_image))
        .route("/routes/{slug}/edit", get(edit_form).post(edit_route))
        .route("/routes/{slug}/delete", post(delete_route))
        .route("/routes/{slug}/report", post(report_route))
        .route("/users/{username}", get(profile))
        .route("/users/{username}/avatar", get(avatar))
        .route("/dashboard", get(dashboard))
        .route("/account", get(account_form).post(account_update))
        .route("/account/resend-verification", post(resend_verification))
        .route("/account/delete", post(delete_account))
        .route("/admin", get(admin))
        .route("/admin/users/{id}/contributor", post(admin_contributor))
        .route("/admin/reports/{id}", post(admin_report))
        .nest_service("/static", ServeDir::new("static"))
        .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
        // Log the path only: query strings can carry a rider's rounded location.
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|req: &axum::http::Request<Body>| {
                    tracing::info_span!(
                        "request",
                        id = %Uuid::new_v4().simple(),
                        method = %req.method(),
                        path = %req.uri().path(),
                    )
                })
                .on_response(
                    DefaultOnResponse::new()
                        .level(Level::INFO)
                        .latency_unit(LatencyUnit::Millis),
                ),
        )
        .with_state(state)
}

async fn home(State(state): State<Shared>, headers: HeaderMap) -> WebResult {
    let v = viewer(&state, &headers).await;
    let routes: Vec<RouteRecord> = sqlx::query_as(
        "SELECT * FROM routes WHERE visibility='public' ORDER BY created_at DESC LIMIT 4",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(AppError::internal)?;
    let mut data = base_context(&state, &v, "Find your next ride", "home");
    data["routes"] = json!(routes.iter().map(route_json).collect::<Vec<_>>());
    data["featured"] = routes.first().map(route_json).unwrap_or(Value::Null);
    render(&state, v, data)
}

#[derive(Default, Deserialize)]
struct ExploreQuery {
    q: Option<String>,
    region: Option<String>,
    cycling_type: Option<String>,
    difficulty: Option<String>,
    min_km: Option<f64>,
    max_km: Option<f64>,
    sort: Option<String>,
    page: Option<i64>,
    near: Option<String>,
}
async fn explore(
    State(state): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<ExploreQuery>,
) -> WebResult {
    let v = viewer(&state, &headers).await;
    let query = q.q.unwrap_or_default().trim().to_owned();
    let region = q.region.unwrap_or_default().trim().to_owned();
    let cycling_type = q.cycling_type.unwrap_or_default();
    let difficulty = q.difficulty.unwrap_or_default();
    let min_km = q.min_km.unwrap_or(0.0).max(0.0);
    let max_km = q.max_km.unwrap_or(100_000.0).max(min_km);
    let page = q.page.unwrap_or(1).clamp(1, 10_000);
    let sort = q.sort.unwrap_or_default();
    let near = q.near.as_deref().and_then(nearby::parse_near);
    let nearby_mode = near.is_some() && (sort.is_empty() || sort == "nearest");
    let mut data = base_context(&state, &v, "Explore routes", "explore");
    data["filters"] = json!({"q":query,"region":region,"cycling_type":cycling_type,"difficulty":difficulty,"min_km":if min_km>0.0 {Some(min_km)}else{None},"max_km":if max_km<100_000.0{Some(max_km)}else{None},"popular":sort=="popular","nearest":nearby_mode});
    data["nearby"] = json!(nearby_mode);
    if let Some(near) = near {
        data["near"] = json!(format!("{},{}", near.lat, near.lon));
    }
    if let (true, Some(near)) = (nearby_mode, near) {
        let b = nearby::search_box(near, nearby::RADIUS_M);
        let candidates: Vec<RouteRecord> = sqlx::query_as("SELECT * FROM routes WHERE visibility='public' AND (?='' OR title LIKE '%'||?||'%') AND (?='' OR region LIKE '%'||?||'%') AND (?='' OR cycling_type=?) AND (?='' OR difficulty=?) AND distance_m>=? AND distance_m<=? AND min_lat<=? AND max_lat>=? AND min_lon<=? AND max_lon>=?")
            .bind(&query).bind(&query).bind(&region).bind(&region).bind(&cycling_type).bind(&cycling_type).bind(&difficulty).bind(&difficulty).bind(min_km*1000.0).bind(max_km*1000.0)
            .bind(b.max_lat).bind(b.min_lat).bind(b.max_lon).bind(b.min_lon)
            .fetch_all(&state.pool).await.map_err(AppError::internal)?;
        let ranked = nearby::rank(candidates, |r| r.geometry_json.as_str(), near);
        data["routes"] = json!(ranked
            .iter()
            .map(|(r, d)| {
                let mut route = route_json(r);
                route["away_km"] = json!(format!("{:.1}", d / 1000.0));
                route
            })
            .collect::<Vec<_>>());
        data["pins_json"] = json!(pins_json(ranked.iter().map(|(r, _)| r)));
        data["count"] = json!(ranked.len());
        data["page_number"] = json!(1);
        return render(&state, v, data);
    }
    let sql = if sort == "popular" {
        "SELECT * FROM routes WHERE visibility='public' AND (?='' OR title LIKE '%'||?||'%') AND (?='' OR region LIKE '%'||?||'%') AND (?='' OR cycling_type=?) AND (?='' OR difficulty=?) AND distance_m>=? AND distance_m<=? ORDER BY download_count DESC, created_at DESC LIMIT 12 OFFSET ?"
    } else {
        "SELECT * FROM routes WHERE visibility='public' AND (?='' OR title LIKE '%'||?||'%') AND (?='' OR region LIKE '%'||?||'%') AND (?='' OR cycling_type=?) AND (?='' OR difficulty=?) AND distance_m>=? AND distance_m<=? ORDER BY created_at DESC LIMIT 12 OFFSET ?"
    };
    let routes: Vec<RouteRecord> = sqlx::query_as(sql)
        .bind(&query)
        .bind(&query)
        .bind(&region)
        .bind(&region)
        .bind(&cycling_type)
        .bind(&cycling_type)
        .bind(&difficulty)
        .bind(&difficulty)
        .bind(min_km * 1000.0)
        .bind(max_km * 1000.0)
        .bind((page - 1) * 12)
        .fetch_all(&state.pool)
        .await
        .map_err(AppError::internal)?;
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM routes WHERE visibility='public' AND (?='' OR title LIKE '%'||?||'%') AND (?='' OR region LIKE '%'||?||'%') AND (?='' OR cycling_type=?) AND (?='' OR difficulty=?) AND distance_m>=? AND distance_m<=?")
        .bind(&query).bind(&query).bind(&region).bind(&region).bind(&cycling_type).bind(&cycling_type).bind(&difficulty).bind(&difficulty).bind(min_km*1000.0).bind(max_km*1000.0)
        .fetch_one(&state.pool).await.map_err(AppError::internal)?;
    data["routes"] = json!(routes.iter().map(route_json).collect::<Vec<_>>());
    data["pins_json"] = json!(pins_json(routes.iter()));
    data["count"] = json!(count);
    data["page_number"] = json!(page);
    data["has_prev"] = json!(page > 1);
    data["has_next"] = json!(page * 12 < count);
    data["prev_page"] = json!(page - 1);
    data["next_page"] = json!(page + 1);
    render(&state, v, data)
}

async fn route_detail(
    State(state): State<Shared>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> WebResult {
    let v = viewer(&state, &headers).await;
    let r = get_route(&state.pool, &slug).await?;
    if !can_view(&r, v.user.as_ref()) {
        return Err(AppError::not_found());
    }
    let author:User=sqlx::query_as("SELECT id,username,email,bio,avatar_key,email_verified_at,contributor_verified_at,is_admin FROM users WHERE id=?")
        .bind(&r.author_id).fetch_one(&state.pool).await.map_err(AppError::internal)?;
    let mut data = base_context(&state, &v, &r.title, "route");
    data["route"] = route_json(&r);
    data["author"] = json!({"username":author.username,"verified":author.contributor_verified_at.is_some(),"avatar":author.avatar_key.is_some()});
    data["can_edit"] = json!(v.user.as_ref().is_some_and(|u| can_edit(&r, u)));
    data["elevation_json"] = json!(r.elevation_json);
    let photos: Vec<String> =
        sqlx::query_scalar("SELECT id FROM route_images WHERE route_id=? ORDER BY position,id")
            .bind(&r.id)
            .fetch_all(&state.pool)
            .await
            .map_err(AppError::internal)?;
    data["photos"] = json!(photos
        .iter()
        .map(|id| format!("/routes/{}/images/{id}", r.slug))
        .collect::<Vec<_>>());
    data["canonical"] = json!(format!("{}/routes/{}", state.base_url, r.slug));
    render(&state, v, data)
}

async fn route_geometry(
    State(state): State<Shared>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> WebResult {
    let v = viewer(&state, &headers).await;
    let r = get_route(&state.pool, &slug).await?;
    if !can_view(&r, v.user.as_ref()) {
        return Err(AppError::not_found());
    }
    Ok((
        [
            (header::CONTENT_TYPE, "application/geo+json"),
            (header::CACHE_CONTROL, "private, max-age=60"),
        ],
        r.geometry_json,
    )
        .into_response())
}

async fn download(
    State(state): State<Shared>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> WebResult {
    let v = viewer(&state, &headers).await;
    let r = get_route(&state.pool, &slug).await?;
    if !can_view(&r, v.user.as_ref()) {
        return Err(AppError::not_found());
    }
    let owner = v.user.as_ref().is_some_and(|u| can_edit(&r, u));
    let key = if owner {
        &r.gpx_key
    } else {
        r.public_gpx_key.as_ref().unwrap_or(&r.gpx_key)
    };
    let bytes = tokio::fs::read(state.storage.join(key))
        .await
        .map_err(AppError::internal)?;
    // Owners and admins fetching the original are not riders discovering the route.
    if !owner {
        sqlx::query("UPDATE routes SET download_count=download_count+1 WHERE id=?")
            .bind(&r.id)
            .execute(&state.pool)
            .await
            .map_err(AppError::internal)?;
    }
    tracing::info!(route_id = %r.id, original = key == &r.gpx_key, counted = !owner, "route downloaded");
    let filename = format!("{}.gpx", r.slug);
    Ok((
        [
            (header::CONTENT_TYPE, "application/gpx+xml".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
            (header::CACHE_CONTROL, "private, no-store".to_string()),
        ],
        Body::from(bytes),
    )
        .into_response())
}

async fn route_cover(
    State(state): State<Shared>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> WebResult {
    let v = viewer(&state, &headers).await;
    let r = get_route(&state.pool, &slug).await?;
    if !can_view(&r, v.user.as_ref()) {
        return Err(AppError::not_found());
    }
    let key = r.cover_key.ok_or_else(AppError::not_found)?;
    serve_image(&state, &key).await
}
async fn route_image(
    State(state): State<Shared>,
    headers: HeaderMap,
    Path((slug, id)): Path<(String, String)>,
) -> WebResult {
    let v = viewer(&state, &headers).await;
    let r = get_route(&state.pool, &slug).await?;
    if !can_view(&r, v.user.as_ref()) {
        return Err(AppError::not_found());
    }
    let key: Option<String> =
        sqlx::query_scalar("SELECT image_key FROM route_images WHERE id=? AND route_id=?")
            .bind(id)
            .bind(&r.id)
            .fetch_optional(&state.pool)
            .await
            .map_err(AppError::internal)?;
    serve_image(&state, &key.ok_or_else(AppError::not_found)?).await
}
async fn avatar(State(state): State<Shared>, Path(username): Path<String>) -> WebResult {
    let user:User=sqlx::query_as("SELECT id,username,email,bio,avatar_key,email_verified_at,contributor_verified_at,is_admin FROM users WHERE username=?")
        .bind(username).fetch_optional(&state.pool).await.map_err(AppError::internal)?.ok_or_else(AppError::not_found)?;
    serve_image(&state, &user.avatar_key.ok_or_else(AppError::not_found)?).await
}
async fn serve_image(state: &AppState, key: &str) -> WebResult {
    let bytes = tokio::fs::read(state.storage.join(key))
        .await
        .map_err(AppError::internal)?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/webp"),
            (header::CACHE_CONTROL, "private, max-age=3600"),
        ],
        Body::from(bytes),
    )
        .into_response())
}

async fn profile(
    State(state): State<Shared>,
    headers: HeaderMap,
    Path(username): Path<String>,
) -> WebResult {
    let v = viewer(&state, &headers).await;
    let user:User=sqlx::query_as("SELECT id,username,email,bio,avatar_key,email_verified_at,contributor_verified_at,is_admin FROM users WHERE username=?")
        .bind(username).fetch_optional(&state.pool).await.map_err(AppError::internal)?.ok_or_else(AppError::not_found)?;
    let routes:Vec<RouteRecord>=sqlx::query_as("SELECT * FROM routes WHERE author_id=? AND visibility='public' ORDER BY created_at DESC LIMIT 48")
        .bind(&user.id).fetch_all(&state.pool).await.map_err(AppError::internal)?;
    let mut data = base_context(&state, &v, &user.username, "profile");
    data["profile"] = json!({"username":user.username,"bio":user.bio,"avatar":user.avatar_key.is_some(),"verified":user.contributor_verified_at.is_some(),"count":routes.len()});
    data["routes"] = json!(routes.iter().map(route_json).collect::<Vec<_>>());
    render(&state, v, data)
}

async fn register_form(State(state): State<Shared>, headers: HeaderMap) -> WebResult {
    let v = viewer(&state, &headers).await;
    let data = base_context(&state, &v, "Create an account", "register");
    render(&state, v, data)
}
async fn login_form(State(state): State<Shared>, headers: HeaderMap) -> WebResult {
    let v = viewer(&state, &headers).await;
    let data = base_context(&state, &v, "Sign in", "login");
    render(&state, v, data)
}
async fn register(
    State(state): State<Shared>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> WebResult {
    let v = viewer(&state, &headers).await;
    csrf(&headers, &form, &v, &state)?;
    honeypot(&form)?;
    let username = val(&form, "username").trim().to_ascii_lowercase();
    let email = val(&form, "email").trim().to_ascii_lowercase();
    let password = val(&form, "password");
    if username.len() < 3
        || username.len() > 32
        || !username
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        return Err(AppError::bad(
            "Username must be 3–32 lowercase letters, numbers or underscores",
        ));
    }
    email::validate(&email).map_err(AppError::bad)?;
    if password.len() < 12 || password.len() > 128 {
        return Err(AppError::bad("Password must be 12–128 characters"));
    }
    let id = Uuid::new_v4().to_string();
    let hash = auth::password_hash(password);
    let mut tx = state.pool.begin().await.map_err(AppError::internal)?;
    sqlx::query("INSERT INTO users(id,username,email,password_hash) VALUES (?,?,?,?)")
        .bind(&id)
        .bind(&username)
        .bind(&email)
        .bind(hash)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::bad("Username or email is already in use"))?;
    let mut mail_available = true;
    if state.brevo_api_key.is_some() {
        let quota=sqlx::query("INSERT INTO email_send_quota(day,sent) VALUES (date('now'),1) ON CONFLICT(day) DO UPDATE SET sent=sent+1 WHERE sent<250")
            .execute(&mut *tx).await.map_err(AppError::internal)?;
        mail_available = quota.rows_affected() > 0;
    }
    // Verification is optional, so a full mail quota only skips the email, never the account.
    if mail_available {
        let verification = auth::random_token();
        let expires = (chrono::Utc::now() + chrono::Duration::days(2))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        sqlx::query(
            "INSERT INTO email_verifications(token_hash,user_id,expires_at) VALUES (?,?,?)",
        )
        .bind(auth::token_hash(&verification))
        .bind(&id)
        .bind(expires)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
        send_verification(&state, &email, &verification).await?;
    } else {
        tracing::warn!(user_id = %id, "daily verification email quota reached, email skipped");
    }
    tx.commit().await.map_err(AppError::internal)?;
    let (token, _) = auth::create_session(&state.pool, &id)
        .await
        .map_err(AppError::internal)?;
    tracing::info!(user_id = %id, "account registered");
    let notice = if mail_available {
        "Welcome! Verify your email any time to lift the daily route limit"
    } else {
        "Welcome! Our verification mail is full today. Resend it from Account settings tomorrow"
    };
    let mut response = msg_redirect("/dashboard", notice);
    response.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_str(&auth::session_cookie(&token, state.secure_cookie))
            .map_err(AppError::internal)?,
    );
    Ok(response)
}
async fn send_verification(state: &AppState, email: &str, token: &str) -> Result<(), AppError> {
    let link = format!("{}/verify/{}", state.base_url, token);
    if let (Some(api), Some(from)) = (&state.brevo_api_key, &state.mail_from) {
        let result=reqwest::Client::new().post("https://api.brevo.com/v3/smtp/email")
            .header("api-key",api).json(&json!({"sender":{"name":"Over There","email":from},"to":[{"email":email}],"subject":"Verify your Over There email","textContent":format!("Verify your email: {link}\nThis link expires in 48 hours.")}))
            .send().await.map_err(AppError::internal)?;
        if !result.status().is_success() {
            return Err(AppError::internal(format!(
                "email provider returned {}",
                result.status()
            )));
        }
        tracing::info!(provider = "brevo", "verification email sent");
    } else {
        tracing::warn!(%email,%link,"Development email verification link");
    }
    Ok(())
}
async fn verify_email(State(state): State<Shared>, Path(token): Path<String>) -> WebResult {
    if token.len() != 64 {
        return Err(AppError::bad("Invalid verification link"));
    }
    let hash = auth::token_hash(&token);
    let user_id: Option<String> = sqlx::query_scalar(
        "SELECT user_id FROM email_verifications WHERE token_hash=? AND expires_at>datetime('now')",
    )
    .bind(&hash)
    .fetch_optional(&state.pool)
    .await
    .map_err(AppError::internal)?;
    let id = user_id.ok_or_else(|| AppError::bad("Verification link is invalid or expired"))?;
    let mut tx = state.pool.begin().await.map_err(AppError::internal)?;
    sqlx::query("UPDATE users SET email_verified_at=datetime('now') WHERE id=?")
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    sqlx::query("DELETE FROM email_verifications WHERE user_id=?")
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    tx.commit().await.map_err(AppError::internal)?;
    tracing::info!(user_id = %id, "email verified");
    Ok(msg_redirect(
        "/dashboard",
        "Email verified. You can now share routes",
    ))
}
async fn login(
    State(state): State<Shared>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> WebResult {
    let v = viewer(&state, &headers).await;
    csrf(&headers, &form, &v, &state)?;
    let email = val(&form, "email").trim().to_ascii_lowercase();
    let blocked:Option<i64>=sqlx::query_scalar("SELECT failures FROM auth_attempts WHERE email=? AND window_started_at>datetime('now','-15 minutes')")
        .bind(&email).fetch_optional(&state.pool).await.map_err(AppError::internal)?;
    if blocked.unwrap_or(0) >= 10 {
        tracing::warn!("sign-in throttled");
        return Err(AppError(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many sign-in attempts. Try again in 15 minutes".into(),
        ));
    }
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT id,password_hash FROM users WHERE email=?")
            .bind(&email)
            .fetch_optional(&state.pool)
            .await
            .map_err(AppError::internal)?;
    let valid = row
        .as_ref()
        .is_some_and(|(_, hash)| auth::verify_password(val(&form, "password"), hash));
    if !valid {
        sqlx::query("INSERT INTO auth_attempts(email,failures,window_started_at) VALUES (?,1,datetime('now')) ON CONFLICT(email) DO UPDATE SET failures=CASE WHEN window_started_at<datetime('now','-15 minutes') THEN 1 ELSE failures+1 END,window_started_at=CASE WHEN window_started_at<datetime('now','-15 minutes') THEN datetime('now') ELSE window_started_at END")
            .bind(&email).execute(&state.pool).await.map_err(AppError::internal)?;
        tracing::warn!(known_account = row.is_some(), "sign-in failed");
        return Err(AppError(
            StatusCode::UNAUTHORIZED,
            "Email or password is incorrect".into(),
        ));
    }
    let id = row.unwrap().0;
    sqlx::query("DELETE FROM auth_attempts WHERE email=?")
        .bind(&email)
        .execute(&state.pool)
        .await
        .map_err(AppError::internal)?;
    let (token, _) = auth::create_session(&state.pool, &id)
        .await
        .map_err(AppError::internal)?;
    tracing::info!(user_id = %id, "signed in");
    let mut response = redirect("/dashboard");
    response.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_str(&auth::session_cookie(&token, state.secure_cookie))
            .map_err(AppError::internal)?,
    );
    Ok(response)
}
async fn logout(
    State(state): State<Shared>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> WebResult {
    let v = viewer(&state, &headers).await;
    csrf(&headers, &form, &v, &state)?;
    if let Some(token) = auth::cookie(&headers, "ot_session") {
        sqlx::query("DELETE FROM sessions WHERE token_hash=?")
            .bind(auth::token_hash(&token))
            .execute(&state.pool)
            .await
            .map_err(AppError::internal)?;
    }
    if let Some(u) = &v.user {
        tracing::info!(user_id = %u.id, "signed out");
    }
    let mut response = redirect("/");
    response.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_static("ot_session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0"),
    );
    Ok(response)
}

async fn dashboard(State(state): State<Shared>, headers: HeaderMap) -> WebResult {
    let v = viewer(&state, &headers).await;
    let user = require_user(&v)?;
    let routes: Vec<RouteRecord> =
        sqlx::query_as("SELECT * FROM routes WHERE author_id=? ORDER BY created_at DESC")
            .bind(&user.id)
            .fetch_all(&state.pool)
            .await
            .map_err(AppError::internal)?;
    let mut data = base_context(&state, &v, "Your routes", "dashboard");
    data["routes"] = json!(routes.iter().map(route_json).collect::<Vec<_>>());
    data["stats"] = json!({"count":routes.len(),"km":format!("{:.1}",routes.iter().map(|r|r.distance_m).sum::<f64>()/1000.0),
        "public":routes.iter().filter(|r|r.visibility=="public").count(),"private":routes.iter().filter(|r|r.visibility=="private").count(),
        "downloads":routes.iter().map(|r|r.download_count).sum::<i64>()});
    render(&state, v, data)
}

async fn upload_form(State(state): State<Shared>, headers: HeaderMap) -> WebResult {
    let v = viewer(&state, &headers).await;
    require_user(&v)?;
    let data = base_context(&state, &v, "Share a route", "upload");
    render(&state, v, data)
}
async fn multipart_values(
    mut multipart: Multipart,
) -> Result<(HashMap<String, String>, HashMap<String, Vec<u8>>), AppError> {
    let mut text = HashMap::new();
    let mut files = HashMap::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::bad("Invalid upload form"))?
    {
        let name = field.name().unwrap_or("").to_owned();
        if name.is_empty() {
            continue;
        }
        if field.file_name().is_some() {
            let bytes = field
                .bytes()
                .await
                .map_err(|_| AppError::bad("Unable to read uploaded file"))?;
            files.insert(name, bytes.to_vec());
        } else {
            let value = field
                .text()
                .await
                .map_err(|_| AppError::bad("Invalid form field"))?;
            if value.len() > 5_000 {
                return Err(AppError::bad("Form field is too long"));
            }
            text.insert(name, value);
        }
    }
    Ok((text, files))
}
async fn write_asset(state: &AppState, bytes: &[u8], ext: &str) -> Result<String, AppError> {
    let key = format!("{}.{}", Uuid::new_v4(), ext);
    tokio::fs::write(state.storage.join(&key), bytes)
        .await
        .map_err(AppError::internal)?;
    Ok(key)
}
fn optimized_image(bytes: &[u8], max: u32) -> Result<Vec<u8>, AppError> {
    if bytes.is_empty() || bytes.len() > 5 * 1024 * 1024 {
        return Err(AppError::bad("Image must be 5 MB or less"));
    }
    let image = image::load_from_memory(bytes)
        .map_err(|_| AppError::bad("Image must be JPEG, PNG or WebP"))?;
    if image.width() as u64 * image.height() as u64 > 30_000_000 {
        return Err(AppError::bad("Image dimensions are too large"));
    }
    let image = image.resize(max, max, image::imageops::FilterType::Lanczos3);
    let mut out = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut out, ImageFormat::WebP)
        .map_err(AppError::internal)?;
    Ok(out.into_inner())
}
async fn upload(
    State(state): State<Shared>,
    headers: HeaderMap,
    multipart: Multipart,
) -> WebResult {
    let v = viewer(&state, &headers).await;
    let user = require_user(&v)?;
    let (form, mut files) = multipart_values(multipart).await?;
    csrf(&headers, &form, &v, &state)?;
    honeypot(&form)?;
    let verified = user.email_verified_at.is_some();
    // Cheap early rejection; the insert below enforces the limit atomically.
    if !verified {
        let today: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM routes WHERE author_id=? AND created_at>datetime('now','-1 day')",
        )
        .bind(&user.id)
        .fetch_one(&state.pool)
        .await
        .map_err(AppError::internal)?;
        if today >= UNVERIFIED_DAILY_ROUTES {
            return Err(daily_limit_reached());
        }
    }
    let draft = RouteDraft::from_fields(&form).map_err(AppError::bad)?;
    let raw = files
        .remove("gpx")
        .ok_or_else(|| AppError::bad("Choose a GPX file"))?;
    let parsed = gpx::parse(&raw).map_err(AppError::bad)?;
    let sha = format!("{:x}", Sha256::digest(&raw));
    let duplicate: Option<String> =
        sqlx::query_scalar("SELECT slug FROM routes WHERE author_id=? AND gpx_sha256=? LIMIT 1")
            .bind(&user.id)
            .bind(&sha)
            .fetch_optional(&state.pool)
            .await
            .map_err(AppError::internal)?;
    if duplicate.is_some() {
        return Err(AppError::bad("You already uploaded this GPX file"));
    }
    let hide = match val(&form, "hide_endpoints") {
        "250" => 250.0,
        "500" => 500.0,
        "1000" => 1000.0,
        _ => 0.0,
    };
    let public_segments = gpx::public_segments(&parsed.segments, hide).map_err(AppError::bad)?;
    let geometry = gpx::geojson(&public_segments);
    let route_box = nearby::bbox(&geometry);
    let id = Uuid::new_v4().to_string();
    let slug = route::slugify(&draft.title, &id);
    let gpx_key = write_asset(&state, &raw, "gpx").await?;
    let public_gpx_key = if hide > 0.0 {
        Some(write_asset(&state, &gpx::redacted_gpx(&public_segments), "gpx").await?)
    } else {
        None
    };
    let cover_key = if let Some(image) = files.remove("cover").filter(|v| !v.is_empty()) {
        Some(write_asset(&state, &optimized_image(&image, 1600)?, "webp").await?)
    } else {
        None
    };
    let mut photo_keys = Vec::new();
    for name in ["photo1", "photo2", "photo3"] {
        if let Some(image) = files.remove(name).filter(|v| !v.is_empty()) {
            photo_keys.push(write_asset(&state, &optimized_image(&image, 1600)?, "webp").await?);
        }
    }
    let elevation = gpx::elevation_profile(&public_segments)
        .as_ref()
        .map(|e| serde_json::to_string(e).unwrap());
    let mut tx = state.pool.begin().await.map_err(AppError::internal)?;
    let result=sqlx::query("INSERT INTO routes(id,author_id,slug,title,description,activity,cycling_type,region,country,difficulty,tags,visibility,gpx_key,public_gpx_key,gpx_sha256,hide_endpoints_m,geometry_json,elevation_json,distance_m,elevation_gain_m,elevation_loss_m,duration_s,cover_key,min_lat,max_lat,min_lon,max_lon) SELECT ?,?,?,?,?,'cycling',?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,? WHERE ? OR (SELECT count(*) FROM routes WHERE author_id=? AND created_at>datetime('now','-1 day'))<?")
        .bind(&id).bind(&user.id).bind(&slug).bind(&draft.title).bind(&draft.description)
        .bind(&draft.cycling_type).bind(&draft.region).bind(&draft.country)
        .bind(&draft.difficulty).bind(&draft.tags).bind(draft.visibility_str())
        .bind(&gpx_key).bind(&public_gpx_key).bind(&sha).bind(hide).bind(&geometry).bind(&elevation)
        .bind(parsed.distance_m).bind(parsed.gain_m).bind(parsed.loss_m).bind(parsed.duration_s).bind(&cover_key)
        .bind(route_box.map(|b| b.min_lat)).bind(route_box.map(|b| b.max_lat)).bind(route_box.map(|b| b.min_lon)).bind(route_box.map(|b| b.max_lon))
        .bind(verified).bind(&user.id).bind(UNVERIFIED_DAILY_ROUTES)
        .execute(&mut *tx).await;
    if !matches!(result, Ok(ref done) if done.rows_affected() == 1) {
        let written = [Some(gpx_key), public_gpx_key, cover_key]
            .into_iter()
            .flatten();
        remove_assets(&state, written.chain(photo_keys)).await;
        return Err(match result {
            Err(e) => AppError::internal(e),
            Ok(_) => daily_limit_reached(),
        });
    }
    for (position, key) in photo_keys.iter().enumerate() {
        sqlx::query("INSERT INTO route_images(id,route_id,image_key,position) VALUES (?,?,?,?)")
            .bind(Uuid::new_v4().to_string())
            .bind(&id)
            .bind(key)
            .bind(position as i64)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?;
    }
    tx.commit().await.map_err(AppError::internal)?;
    tracing::info!(
        route_id = %id,
        user_id = %user.id,
        visibility = draft.visibility_str(),
        points = parsed.segments.iter().map(Vec::len).sum::<usize>(),
        segments = parsed.segments.len(),
        distance_m = parsed.distance_m.round(),
        hide_endpoints_m = hide,
        photos = photo_keys.len(),
        "route uploaded"
    );
    Ok(redirect(&format!("/routes/{slug}")))
}

async fn edit_form(
    State(state): State<Shared>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> WebResult {
    let v = viewer(&state, &headers).await;
    let user = require_user(&v)?;
    let r = get_route(&state.pool, &slug).await?;
    if !can_edit(&r, user) {
        return Err(AppError::forbidden());
    }
    let mut data = base_context(&state, &v, "Edit route", "edit");
    data["route"] = route_json(&r);
    render(&state, v, data)
}
async fn edit_route(
    State(state): State<Shared>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Form(form): Form<HashMap<String, String>>,
) -> WebResult {
    let v = viewer(&state, &headers).await;
    let user = require_user(&v)?;
    csrf(&headers, &form, &v, &state)?;
    let r = get_route(&state.pool, &slug).await?;
    if !can_edit(&r, user) {
        return Err(AppError::forbidden());
    }
    let draft = RouteDraft::from_fields(&form).map_err(AppError::bad)?;
    sqlx::query("UPDATE routes SET title=?,description=?,cycling_type=?,region=?,country=?,difficulty=?,tags=?,visibility=?,updated_at=datetime('now') WHERE id=?")
        .bind(&draft.title).bind(&draft.description).bind(&draft.cycling_type)
        .bind(&draft.region).bind(&draft.country).bind(&draft.difficulty)
        .bind(&draft.tags).bind(draft.visibility_str()).bind(&r.id)
        .execute(&state.pool).await.map_err(AppError::internal)?;
    tracing::info!(route_id = %r.id, user_id = %user.id, visibility = draft.visibility_str(), "route updated");
    Ok(redirect(&format!("/routes/{slug}")))
}
async fn delete_route(
    State(state): State<Shared>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Form(form): Form<HashMap<String, String>>,
) -> WebResult {
    let v = viewer(&state, &headers).await;
    let user = require_user(&v)?;
    csrf(&headers, &form, &v, &state)?;
    let r = get_route(&state.pool, &slug).await?;
    if !can_edit(&r, user) {
        return Err(AppError::forbidden());
    }
    let photos: Vec<String> =
        sqlx::query_scalar("SELECT image_key FROM route_images WHERE route_id=?")
            .bind(&r.id)
            .fetch_all(&state.pool)
            .await
            .map_err(AppError::internal)?;
    sqlx::query("DELETE FROM routes WHERE id=?")
        .bind(&r.id)
        .execute(&state.pool)
        .await
        .map_err(AppError::internal)?;
    let files = [Some(r.gpx_key), r.public_gpx_key, r.cover_key]
        .into_iter()
        .flatten();
    remove_assets(&state, files.chain(photos)).await;
    tracing::info!(route_id = %r.id, user_id = %user.id, "route deleted");
    Ok(redirect("/dashboard"))
}

async fn account_form(State(state): State<Shared>, headers: HeaderMap) -> WebResult {
    let v = viewer(&state, &headers).await;
    let u = require_user(&v)?;
    let mut data = base_context(&state, &v, "Account settings", "account");
    data["account"] = json!({"username":u.username,"email":u.email,"bio":u.bio,"avatar":u.avatar_key.is_some(),"verified":u.email_verified_at.is_some()});
    render(&state, v, data)
}
async fn account_update(
    State(state): State<Shared>,
    headers: HeaderMap,
    multipart: Multipart,
) -> WebResult {
    let v = viewer(&state, &headers).await;
    let u = require_user(&v)?;
    let (form, mut files) = multipart_values(multipart).await?;
    csrf(&headers, &form, &v, &state)?;
    let bio = val(&form, "bio").trim();
    if bio.len() > 300 {
        return Err(AppError::bad("Biography must be 300 characters or less"));
    }
    let avatar_key = if let Some(image) = files.remove("avatar").filter(|v| !v.is_empty()) {
        Some(write_asset(&state, &optimized_image(&image, 512)?, "webp").await?)
    } else {
        None
    };
    if let Some(key) = &avatar_key {
        sqlx::query("UPDATE users SET bio=?,avatar_key=? WHERE id=?")
            .bind(bio)
            .bind(key)
            .bind(&u.id)
            .execute(&state.pool)
            .await
            .map_err(AppError::internal)?;
        remove_assets(&state, u.avatar_key.clone()).await;
    } else {
        sqlx::query("UPDATE users SET bio=? WHERE id=?")
            .bind(bio)
            .bind(&u.id)
            .execute(&state.pool)
            .await
            .map_err(AppError::internal)?;
    }
    tracing::info!(user_id = %u.id, avatar_changed = avatar_key.is_some(), "account updated");
    Ok(msg_redirect("/account", "Account updated"))
}

async fn delete_account(
    State(state): State<Shared>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> WebResult {
    let v = viewer(&state, &headers).await;
    let u = require_user(&v)?;
    csrf(&headers, &form, &v, &state)?;
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id=?")
        .bind(&u.id)
        .fetch_one(&state.pool)
        .await
        .map_err(AppError::internal)?;
    if !auth::verify_password(val(&form, "password"), &hash) {
        return Err(AppError(
            StatusCode::UNAUTHORIZED,
            "Password is incorrect".into(),
        ));
    }
    let routes: Vec<(String, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT gpx_key,public_gpx_key,cover_key FROM routes WHERE author_id=?")
            .bind(&u.id)
            .fetch_all(&state.pool)
            .await
            .map_err(AppError::internal)?;
    let photos: Vec<String> = sqlx::query_scalar(
        "SELECT i.image_key FROM route_images i JOIN routes r ON r.id=i.route_id WHERE r.author_id=?",
    )
    .bind(&u.id)
    .fetch_all(&state.pool)
    .await
    .map_err(AppError::internal)?;
    // Cascades remove sessions, verification links, routes, their photos and reports.
    sqlx::query("DELETE FROM users WHERE id=?")
        .bind(&u.id)
        .execute(&state.pool)
        .await
        .map_err(AppError::internal)?;
    let route_count = routes.len();
    let files = routes
        .into_iter()
        .flat_map(|(gpx, public, cover)| [Some(gpx), public, cover])
        .flatten()
        .chain(photos)
        .chain(u.avatar_key.clone());
    remove_assets(&state, files).await;
    tracing::info!(user_id = %u.id, routes = route_count, "account deleted");
    let mut response = msg_redirect("/", "Your account was deleted");
    response.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_static("ot_session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0"),
    );
    Ok(response)
}

async fn resend_verification(
    State(state): State<Shared>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> WebResult {
    let v = viewer(&state, &headers).await;
    let u = require_user(&v)?;
    csrf(&headers, &form, &v, &state)?;
    if u.email_verified_at.is_some() {
        return Ok(redirect("/account"));
    }
    let recent: i64 = sqlx::query_scalar("SELECT count(*) FROM email_verifications WHERE user_id=? AND sent_at>datetime('now','-10 minutes')")
        .bind(&u.id).fetch_one(&state.pool).await.map_err(AppError::internal)?;
    if recent > 0 {
        return Err(AppError(
            StatusCode::TOO_MANY_REQUESTS,
            "Please wait 10 minutes before requesting another link".into(),
        ));
    }
    let token = auth::random_token();
    let expires = (chrono::Utc::now() + chrono::Duration::days(2))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let mut tx = state.pool.begin().await.map_err(AppError::internal)?;
    if state.brevo_api_key.is_some() {
        let quota=sqlx::query("INSERT INTO email_send_quota(day,sent) VALUES (date('now'),1) ON CONFLICT(day) DO UPDATE SET sent=sent+1 WHERE sent<250")
            .execute(&mut *tx).await.map_err(AppError::internal)?;
        if quota.rows_affected() == 0 {
            tracing::warn!("daily verification email quota reached");
            return Err(AppError(
                StatusCode::TOO_MANY_REQUESTS,
                "Verification mail is temporarily full for today. Try again tomorrow".into(),
            ));
        }
    }
    sqlx::query("DELETE FROM email_verifications WHERE user_id=?")
        .bind(&u.id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    sqlx::query("INSERT INTO email_verifications(token_hash,user_id,expires_at) VALUES (?,?,?)")
        .bind(auth::token_hash(&token))
        .bind(&u.id)
        .bind(expires)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    send_verification(&state, &u.email, &token).await?;
    tx.commit().await.map_err(AppError::internal)?;
    tracing::info!(user_id = %u.id, "verification link resent");
    Ok(msg_redirect("/account", "A new verification link was sent"))
}

async fn report_route(
    State(state): State<Shared>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Form(form): Form<HashMap<String, String>>,
) -> WebResult {
    let v = viewer(&state, &headers).await;
    csrf(&headers, &form, &v, &state)?;
    let r = get_route(&state.pool, &slug).await?;
    if r.visibility == "private" {
        return Err(AppError::not_found());
    }
    let reason = val(&form, "reason");
    if !valid_choice(reason, &["misleading", "copyright", "unsafe", "other"]) {
        return Err(AppError::bad("Choose a report reason"));
    }
    let details = val(&form, "details").trim();
    if details.len() > 1000 {
        return Err(AppError::bad("Report details are too long"));
    }
    let report_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO route_reports(id,route_id,reporter_id,reason,details) VALUES (?,?,?,?,?)",
    )
    .bind(&report_id)
    .bind(&r.id)
    .bind(v.user.as_ref().map(|u| u.id.as_str()))
    .bind(reason)
    .bind(details)
    .execute(&state.pool)
    .await
    .map_err(AppError::internal)?;
    tracing::info!(%report_id, route_id = %r.id, reason, signed_in = v.user.is_some(), "route reported");
    Ok(msg_redirect(
        &format!("/routes/{slug}"),
        "Report received. Thank you",
    ))
}
async fn admin(State(state): State<Shared>, headers: HeaderMap) -> WebResult {
    let v = viewer(&state, &headers).await;
    let u = require_user(&v)?;
    if u.is_admin == 0 {
        return Err(AppError::forbidden());
    }
    let users:Vec<User>=sqlx::query_as("SELECT id,username,email,bio,avatar_key,email_verified_at,contributor_verified_at,is_admin FROM users ORDER BY created_at DESC LIMIT 100")
        .fetch_all(&state.pool).await.map_err(AppError::internal)?;
    let reports:Vec<(String,String,String,String,String,String)>=sqlx::query_as("SELECT rr.id,r.slug,r.title,rr.reason,rr.details,rr.status FROM route_reports rr JOIN routes r ON r.id=rr.route_id ORDER BY rr.created_at DESC LIMIT 100")
        .fetch_all(&state.pool).await.map_err(AppError::internal)?;
    let total_users: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(&state.pool)
        .await
        .map_err(AppError::internal)?;
    let total_routes: i64 = sqlx::query_scalar("SELECT count(*) FROM routes")
        .fetch_one(&state.pool)
        .await
        .map_err(AppError::internal)?;
    let open_reports: i64 =
        sqlx::query_scalar("SELECT count(*) FROM route_reports WHERE status='open'")
            .fetch_one(&state.pool)
            .await
            .map_err(AppError::internal)?;
    let mut data = base_context(&state, &v, "Administration", "admin");
    data["stats"] = json!({"users":total_users,"routes":total_routes,"reports":open_reports});
    data["users"]=json!(users.iter().map(|u|json!({"id":u.id,"username":u.username,"email":u.email,"email_verified":u.email_verified_at.is_some(),"contributor":u.contributor_verified_at.is_some()})).collect::<Vec<_>>());
    data["reports"]=json!(reports.iter().map(|(id,slug,title,reason,details,status)|json!({"id":id,"slug":slug,"title":title,"reason":reason,"details":details,"status":status,"open":status=="open"})).collect::<Vec<_>>());
    render(&state, v, data)
}
async fn admin_contributor(
    State(state): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<HashMap<String, String>>,
) -> WebResult {
    let v = viewer(&state, &headers).await;
    let u = require_user(&v)?;
    if u.is_admin == 0 {
        return Err(AppError::forbidden());
    }
    csrf(&headers, &form, &v, &state)?;
    let verified = val(&form, "verified") == "true";
    sqlx::query("UPDATE users SET contributor_verified_at=CASE WHEN ? THEN datetime('now') ELSE NULL END WHERE id=? AND email_verified_at IS NOT NULL")
        .bind(verified).bind(&id).execute(&state.pool).await.map_err(AppError::internal)?;
    tracing::info!(admin_id = %u.id, user_id = %id, verified, "contributor badge changed");
    Ok(redirect("/admin"))
}
async fn admin_report(
    State(state): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<HashMap<String, String>>,
) -> WebResult {
    let v = viewer(&state, &headers).await;
    let u = require_user(&v)?;
    if u.is_admin == 0 {
        return Err(AppError::forbidden());
    }
    csrf(&headers, &form, &v, &state)?;
    let action = val(&form, "action");
    let route_id: Option<String> =
        sqlx::query_scalar("SELECT route_id FROM route_reports WHERE id=?")
            .bind(&id)
            .fetch_optional(&state.pool)
            .await
            .map_err(AppError::internal)?;
    let route_id = route_id.ok_or_else(AppError::not_found)?;
    match action {
        "dismiss" => {
            sqlx::query("UPDATE route_reports SET status='dismissed' WHERE id=?")
                .bind(&id)
                .execute(&state.pool)
                .await
                .map_err(AppError::internal)?;
        }
        "remove" => {
            let mut tx = state.pool.begin().await.map_err(AppError::internal)?;
            sqlx::query(
                "UPDATE routes SET visibility='private',updated_at=datetime('now') WHERE id=?",
            )
            .bind(&route_id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?;
            sqlx::query("UPDATE route_reports SET status='removed' WHERE id=?")
                .bind(&id)
                .execute(&mut *tx)
                .await
                .map_err(AppError::internal)?;
            tx.commit().await.map_err(AppError::internal)?;
        }
        _ => return Err(AppError::bad("Unknown moderation action")),
    }
    tracing::info!(admin_id = %u.id, report_id = %id, %route_id, action, "report resolved");
    Ok(redirect("/admin"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::nearby;
    use axum::{body::to_bytes, http::Request};
    use tower::ServiceExt;

    async fn setup() -> (Router, Shared, tempfile::TempDir) {
        setup_with_mail(None).await
    }
    async fn setup_with_mail(brevo_api_key: Option<String>) -> (Router, Shared, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_url = format!("sqlite://{}", dir.path().join("test.db").display());
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::from_str(&db_url)
                    .unwrap()
                    .create_if_missing(true),
            )
            .await
            .unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        let storage = dir.path().join("files");
        tokio::fs::create_dir_all(&storage).await.unwrap();
        let mut templates = Handlebars::new();
        handlebars::handlebars_helper!(eq: |a: str, b: str| a == b);
        templates.register_helper("eq", Box::new(eq));
        templates
            .register_template_string("page", include_str!("../../templates/page.hbs"))
            .unwrap();
        let state = Arc::new(AppState {
            pool,
            templates,
            storage,
            base_url: "http://localhost".into(),
            map_style: "/static/map-style.json".into(),
            secure_cookie: false,
            brevo_api_key,
            mail_from: None,
        });
        (router(state.clone()), state, dir)
    }
    async fn send(
        app: &Router,
        method: &str,
        uri: &str,
        body: Body,
        content_type: Option<&str>,
        cookie: Option<&str>,
    ) -> Response {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(ct) = content_type {
            req = req.header(header::CONTENT_TYPE, ct);
        }
        if let Some(c) = cookie {
            req = req.header(header::COOKIE, c);
        }
        app.clone().oneshot(req.body(body).unwrap()).await.unwrap()
    }
    fn field(html: &str, name: &str) -> String {
        let marker = format!("name=\"{name}\" value=\"");
        html.split(&marker)
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
            .to_owned()
    }
    const TAIPEI_GPX: &str = "<gpx><trk><trkseg><trkpt lat=\"25\" lon=\"121\"><ele>5</ele></trkpt><trkpt lat=\"25.01\" lon=\"121.01\"><ele>25</ele></trkpt><trkpt lat=\"25.02\" lon=\"121.02\"><ele>40</ele></trkpt><trkpt lat=\"25.03\" lon=\"121.03\"><ele>20</ele></trkpt></trkseg></trk></gpx>";

    async fn body_text(response: Response) -> String {
        String::from_utf8(
            to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap()
    }

    async fn signed_in_rider(app: &Router, state: &Shared) -> String {
        let session = unverified_rider(app).await;
        sqlx::query(
            "UPDATE users SET email_verified_at=datetime('now') WHERE username='test_rider'",
        )
        .execute(&state.pool)
        .await
        .unwrap();
        session
    }

    async fn register_rider(app: &Router, extra: &str) -> Response {
        let register = send(app, "GET", "/register", Body::empty(), None, None).await;
        assert_eq!(register.status(), StatusCode::OK);
        let cookie = register.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let csrf = field(&body_text(register).await, "csrf");
        let form = format!("csrf={csrf}&username=test_rider&email=test%40example.com&password=a+very+long+test+password{extra}");
        send(
            app,
            "POST",
            "/register",
            Body::from(form),
            Some("application/x-www-form-urlencoded"),
            Some(&cookie),
        )
        .await
    }

    async fn unverified_rider(app: &Router) -> String {
        let registered = register_rider(app, "").await;
        assert_eq!(registered.status(), StatusCode::SEE_OTHER);
        registered.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned()
    }

    async fn upload_route(
        app: &Router,
        session: &str,
        title: &str,
        gpx: &str,
        visibility: &str,
        difficulty: &str,
    ) -> String {
        let uploaded = post_upload(app, session, title, gpx, visibility, difficulty, "").await;
        assert_eq!(uploaded.status(), StatusCode::SEE_OTHER);
        uploaded.headers()[header::LOCATION]
            .to_str()
            .unwrap()
            .to_owned()
    }

    async fn post_upload(
        app: &Router,
        session: &str,
        title: &str,
        gpx: &str,
        visibility: &str,
        difficulty: &str,
        website: &str,
    ) -> Response {
        let upload_page = send(app, "GET", "/upload", Body::empty(), None, Some(session)).await;
        assert_eq!(upload_page.status(), StatusCode::OK);
        let csrf = field(&body_text(upload_page).await, "csrf");
        let mut multipart = String::new();
        for (k, value) in [
            ("csrf", csrf.as_str()),
            ("title", title),
            ("description", "A synthetic route"),
            ("cycling_type", "road"),
            ("region", "Taipei"),
            ("country", "Taiwan"),
            ("difficulty", difficulty),
            ("tags", "river"),
            ("visibility", visibility),
            ("hide_endpoints", "250"),
            ("website", website),
        ] {
            multipart.push_str(&format!("--test-boundary\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{value}\r\n"));
        }
        multipart.push_str(&format!("--test-boundary\r\nContent-Disposition: form-data; name=\"gpx\"; filename=\"ride.gpx\"\r\nContent-Type: application/gpx+xml\r\n\r\n{gpx}\r\n--test-boundary--\r\n"));
        send(
            app,
            "POST",
            "/upload",
            Body::from(multipart),
            Some("multipart/form-data; boundary=test-boundary"),
            Some(session),
        )
        .await
    }

    #[tokio::test]
    async fn unverified_riders_share_up_to_five_routes_a_day() {
        let (app, _state, _dir) = setup().await;
        let session = unverified_rider(&app).await;
        for i in 0..5 {
            let gpx = TAIPEI_GPX.replace("<ele>5</ele>", &format!("<ele>{i}</ele>"));
            upload_route(&app, &session, "Daily Ride", &gpx, "public", "easy").await;
        }
        let gpx = TAIPEI_GPX.replace("<ele>5</ele>", "<ele>9</ele>");
        let sixth = post_upload(&app, &session, "Daily Ride", &gpx, "public", "easy", "").await;
        assert_eq!(sixth.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn concurrent_uploads_cannot_pass_the_daily_limit() {
        let (app, state, _dir) = setup().await;
        let session = unverified_rider(&app).await;
        let mut uploads = tokio::task::JoinSet::new();
        for i in 0..8 {
            let (app, session) = (app.clone(), session.clone());
            let gpx = TAIPEI_GPX.replace("<ele>5</ele>", &format!("<ele>{i}</ele>"));
            uploads.spawn(async move {
                post_upload(&app, &session, "Burst Ride", &gpx, "public", "easy", "")
                    .await
                    .status()
            });
        }
        let statuses = uploads.join_all().await;
        let published = statuses
            .iter()
            .filter(|s| **s == StatusCode::SEE_OTHER)
            .count();
        assert_eq!(published, 5);
        assert_eq!(count(&state, "SELECT count(*) FROM routes").await, 5);
        assert!(statuses
            .iter()
            .all(|s| *s == StatusCode::SEE_OTHER || *s == StatusCode::TOO_MANY_REQUESTS));
    }

    #[tokio::test]
    async fn verified_riders_have_no_daily_route_limit() {
        let (app, state, _dir) = setup().await;
        let session = signed_in_rider(&app, &state).await;
        for i in 0..6 {
            let gpx = TAIPEI_GPX.replace("<ele>5</ele>", &format!("<ele>{i}</ele>"));
            upload_route(&app, &session, "Daily Ride", &gpx, "public", "easy").await;
        }
    }

    #[tokio::test]
    async fn honeypot_rejects_register_and_upload() {
        let (app, state, _dir) = setup().await;
        let bot = register_rider(&app, "&website=spam.example").await;
        assert_eq!(bot.status(), StatusCode::BAD_REQUEST);
        assert_eq!(count(&state, "SELECT count(*) FROM users").await, 0);
        let session = unverified_rider(&app).await;
        let upload = post_upload(
            &app, &session, "Bot Ride", TAIPEI_GPX, "public", "easy", "x",
        )
        .await;
        assert_eq!(upload.status(), StatusCode::BAD_REQUEST);
        assert_eq!(count(&state, "SELECT count(*) FROM routes").await, 0);
    }

    #[tokio::test]
    async fn signup_succeeds_without_email_when_mail_quota_is_full() {
        let (app, state, _dir) = setup_with_mail(Some("test-key".into())).await;
        sqlx::query("INSERT INTO email_send_quota(day,sent) VALUES (date('now'),250)")
            .execute(&state.pool)
            .await
            .unwrap();
        unverified_rider(&app).await;
        assert_eq!(count(&state, "SELECT count(*) FROM users").await, 1);
        assert_eq!(
            count(&state, "SELECT count(*) FROM email_verifications").await,
            0
        );
    }

    #[tokio::test]
    async fn public_pages_render_and_route_privacy_is_enforced() {
        let (app, state, _dir) = setup().await;
        let home = send(&app, "GET", "/", Body::empty(), None, None).await;
        assert_eq!(home.status(), StatusCode::OK);
        let body = to_bytes(home.into_body(), usize::MAX).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("Go a little further"));
        if let Ok(path) = std::env::var("PREVIEW_OUT") {
            let html = String::from_utf8_lossy(&body).replace("/static/", "static/");
            std::fs::write(path, html).unwrap();
        }
        let session = signed_in_rider(&app, &state).await;
        let gpx = TAIPEI_GPX;
        let location =
            upload_route(&app, &session, "Taipei Test Ride", gpx, "private", "easy").await;
        let location = location.as_str();
        let private = send(&app, "GET", location, Body::empty(), None, None).await;
        assert_eq!(private.status(), StatusCode::NOT_FOUND);
        let owner = send(&app, "GET", location, Body::empty(), None, Some(&session)).await;
        assert_eq!(owner.status(), StatusCode::OK);
        let geometry = send(
            &app,
            "GET",
            &format!("{location}/geometry"),
            Body::empty(),
            None,
            None,
        )
        .await;
        assert_eq!(geometry.status(), StatusCode::NOT_FOUND);
        let owner_download = send(
            &app,
            "GET",
            &format!("{location}/download"),
            Body::empty(),
            None,
            Some(&session),
        )
        .await;
        assert_eq!(owner_download.status(), StatusCode::OK);
        let bytes = to_bytes(owner_download.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(bytes.as_ref(), gpx.as_bytes());
    }

    const STYLE: &str = include_str!("../../static/style.css");

    #[test]
    fn stylesheet_colors_live_only_in_root_tokens() {
        assert!(STYLE.starts_with(":root{"));
        let rest = &STYLE[STYLE.find('}').unwrap()..];
        // A color is # plus 3/4/6/8 hex digits; ids like #elevation-chart run on into a name.
        let hex = rest.match_indices('#').find(|(i, _)| {
            let after = &rest[i + 1..];
            let n = after
                .find(|c: char| !c.is_ascii_hexdigit())
                .unwrap_or(after.len());
            let next = after[n..].chars().next().unwrap_or(' ');
            [3, 4, 6, 8].contains(&n) && !(next.is_alphanumeric() || next == '-' || next == '_')
        });
        assert!(
            hex.is_none(),
            "color literal outside :root near {:?}",
            hex.map(|(i, _)| &rest[i..i + 12])
        );
        assert!(!rest.contains("rgb(") && !rest.contains("rgba("));
        for file in [
            include_str!("../../static/route.js"),
            include_str!("../../templates/page.hbs"),
        ] {
            let file = file.to_ascii_uppercase();
            for old in ["#006D77", "#83C5BE", "#EDF6F9", "#FFDDD2", "#E29578"] {
                assert!(!file.contains(old), "old palette color {old} still used");
            }
        }
    }

    fn token(name: &str) -> [f64; 3] {
        let root = &STYLE[..STYLE.find('}').unwrap()];
        let at = root
            .find(&format!("{name}:#"))
            .unwrap_or_else(|| panic!("{name} missing"))
            + name.len()
            + 2;
        let channel =
            |i: usize| u8::from_str_radix(&root[at + i..at + i + 2], 16).unwrap() as f64 / 255.0;
        [channel(0), channel(2), channel(4)]
    }

    fn contrast(a: [f64; 3], b: [f64; 3]) -> f64 {
        let luminance = |c: [f64; 3]| {
            let l = c.map(|v| {
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            });
            0.2126 * l[0] + 0.7152 * l[1] + 0.0722 * l[2]
        };
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn palette_token_pairs_meet_wcag_aa() {
        assert!((contrast([0.0; 3], [1.0; 3]) - 21.0).abs() < 0.01);
        let pairs = [
            ("--black", "--bg"),
            ("--black", "--white"),
            ("--muted", "--white"),
            ("--muted", "--bg"),
            ("--black", "--lime"),
            ("--white", "--black"),
            ("--lime", "--black"),
            ("--danger", "--white"),
            ("--white", "--charcoal"),
        ];
        for (fg, bg) in pairs {
            let ratio = contrast(token(fg), token(bg));
            assert!(ratio >= 4.5, "{fg} on {bg} is {ratio:.2}:1");
        }
    }

    #[test]
    fn focus_ring_wins_over_component_shadows() {
        // .button, .route-card and the account pill set box-shadow at equal or higher specificity.
        assert!(STYLE.contains(":focus-visible{outline:none;box-shadow:var(--ring)!important}"));
    }

    async fn stored_box(state: &Shared) -> (Option<nearby::BBox>, Option<nearby::BBox>) {
        let (geometry, min_lat, max_lat, min_lon, max_lon): (
            String,
            Option<f64>,
            Option<f64>,
            Option<f64>,
            Option<f64>,
        ) = sqlx::query_as("SELECT geometry_json,min_lat,max_lat,min_lon,max_lon FROM routes")
            .fetch_one(&state.pool)
            .await
            .unwrap();
        let stored = min_lat.map(|min_lat| nearby::BBox {
            min_lat,
            max_lat: max_lat.unwrap(),
            min_lon: min_lon.unwrap(),
            max_lon: max_lon.unwrap(),
        });
        (stored, nearby::bbox(&geometry))
    }

    #[tokio::test]
    async fn upload_stores_public_geometry_route_box() {
        let (app, state, _dir) = setup().await;
        let session = signed_in_rider(&app, &state).await;
        upload_route(&app, &session, "Box Ride", TAIPEI_GPX, "public", "easy").await;
        let (stored, expected) = stored_box(&state).await;
        assert!(expected.is_some());
        assert_eq!(stored, expected);
    }

    #[tokio::test]
    async fn backfill_fills_missing_route_boxes() {
        let (app, state, _dir) = setup().await;
        let session = signed_in_rider(&app, &state).await;
        upload_route(&app, &session, "Box Ride", TAIPEI_GPX, "public", "easy").await;
        sqlx::query("UPDATE routes SET min_lat=NULL,max_lat=NULL,min_lon=NULL,max_lon=NULL")
            .execute(&state.pool)
            .await
            .unwrap();
        assert_eq!(routes::backfill_route_boxes(&state.pool).await.unwrap(), 1);
        let (stored, expected) = stored_box(&state).await;
        assert_eq!(stored, expected);
        assert_eq!(routes::backfill_route_boxes(&state.pool).await.unwrap(), 0);
    }

    async fn account_csrf(app: &Router, session: &str) -> String {
        let page = send(app, "GET", "/account", Body::empty(), None, Some(session)).await;
        assert_eq!(page.status(), StatusCode::OK);
        field(&body_text(page).await, "csrf")
    }

    async fn count(state: &Shared, sql: &str) -> i64 {
        sqlx::query_scalar(sql)
            .fetch_one(&state.pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn account_deletion_needs_password_and_removes_routes_and_files() {
        let (app, state, _dir) = setup().await;
        let session = signed_in_rider(&app, &state).await;
        let location =
            upload_route(&app, &session, "Leaving Ride", TAIPEI_GPX, "public", "easy").await;
        let csrf = account_csrf(&app, &session).await;
        let delete = |password: &str| format!("csrf={csrf}&password={password}");
        let form = Some("application/x-www-form-urlencoded");
        let wrong = send(
            &app,
            "POST",
            "/account/delete",
            Body::from(delete("wrong")),
            form,
            Some(&session),
        )
        .await;
        assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(count(&state, "SELECT count(*) FROM users").await, 1);
        let right = send(
            &app,
            "POST",
            "/account/delete",
            Body::from(delete("a+very+long+test+password")),
            form,
            Some(&session),
        )
        .await;
        assert_eq!(right.status(), StatusCode::SEE_OTHER);
        assert_eq!(count(&state, "SELECT count(*) FROM users").await, 0);
        assert_eq!(count(&state, "SELECT count(*) FROM routes").await, 0);
        assert_eq!(count(&state, "SELECT count(*) FROM sessions").await, 0);
        assert_eq!(std::fs::read_dir(&state.storage).unwrap().count(), 0);
        let gone = send(&app, "GET", &location, Body::empty(), None, None).await;
        assert_eq!(gone.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn owner_downloads_are_not_counted() {
        let (app, state, _dir) = setup().await;
        let session = signed_in_rider(&app, &state).await;
        let location =
            upload_route(&app, &session, "Counted Ride", TAIPEI_GPX, "public", "easy").await;
        let download = format!("{location}/download");
        let owner = send(&app, "GET", &download, Body::empty(), None, Some(&session)).await;
        assert_eq!(owner.status(), StatusCode::OK);
        assert_eq!(count(&state, "SELECT download_count FROM routes").await, 0);
        let visitor = send(&app, "GET", &download, Body::empty(), None, None).await;
        assert_eq!(visitor.status(), StatusCode::OK);
        assert_eq!(count(&state, "SELECT download_count FROM routes").await, 1);
    }

    #[tokio::test]
    async fn purge_removes_only_expired_records() {
        let (app, state, _dir) = setup().await;
        signed_in_rider(&app, &state).await;
        for sql in [
            "INSERT INTO sessions(token_hash,user_id,csrf_token,expires_at) SELECT 'old',id,'x',datetime('now','-1 day') FROM users",
            "INSERT INTO email_verifications(token_hash,user_id,expires_at) SELECT 'old',id,datetime('now','-1 day') FROM users",
            "INSERT INTO auth_attempts(email,failures,window_started_at) VALUES ('old@example.com',3,datetime('now','-1 hour')),('new@example.com',3,datetime('now'))",
        ] {
            sqlx::query(sql).execute(&state.pool).await.unwrap();
        }
        let purged = auth::purge_expired(&state.pool).await.unwrap();
        assert_eq!(
            purged,
            auth::Purged {
                sessions: 1,
                verifications: 1,
                attempts: 1
            }
        );
        assert_eq!(count(&state, "SELECT count(*) FROM sessions").await, 1);
        assert_eq!(
            count(&state, "SELECT count(*) FROM email_verifications").await,
            1
        );
        assert_eq!(count(&state, "SELECT count(*) FROM auth_attempts").await, 1);
    }

    #[tokio::test]
    async fn startup_strips_times_from_old_redacted_files() {
        let (app, state, _dir) = setup().await;
        let session = signed_in_rider(&app, &state).await;
        upload_route(&app, &session, "Timed Ride", TAIPEI_GPX, "public", "easy").await;
        let key: String = sqlx::query_scalar("SELECT public_gpx_key FROM routes")
            .fetch_one(&state.pool)
            .await
            .unwrap();
        let old = "<gpx><trk><trkseg><trkpt lat=\"25\" lon=\"121\"><time>2026-01-01T08:00:00Z</time></trkpt><trkpt lat=\"25.01\" lon=\"121.01\"><time>2026-01-01T08:05:00Z</time></trkpt></trkseg></trk></gpx>";
        std::fs::write(state.storage.join(&key), old).unwrap();
        assert_eq!(
            strip_redacted_times(&state.pool, &state.storage)
                .await
                .unwrap(),
            1
        );
        let rewritten = std::fs::read_to_string(state.storage.join(&key)).unwrap();
        assert!(!rewritten.contains("<time>"));
        assert!(rewritten.contains("lat=\"25.01\""));
        assert_eq!(
            strip_redacted_times(&state.pool, &state.storage)
                .await
                .unwrap(),
            0
        );
    }

    /// A synthetic ~1 km line heading north from (lat, lon).
    fn line_gpx(lat: f64, lon: f64) -> String {
        let points: String = (0..4)
            .map(|i| {
                format!(
                    "<trkpt lat=\"{:.4}\" lon=\"{lon:.4}\"><ele>5</ele></trkpt>",
                    lat + i as f64 * 0.003
                )
            })
            .collect();
        format!("<gpx><trk><trkseg>{points}</trkseg></trk></gpx>")
    }

    async fn nearby_fixture() -> (Router, tempfile::TempDir) {
        let (app, state, dir) = setup().await;
        let session = signed_in_rider(&app, &state).await;
        upload_route(
            &app,
            &session,
            "Alpha Near",
            &line_gpx(10.009, 20.0),
            "public",
            "easy",
        )
        .await;
        upload_route(
            &app,
            &session,
            "Bravo Middle",
            &line_gpx(10.09, 20.0),
            "public",
            "hard",
        )
        .await;
        upload_route(
            &app,
            &session,
            "Charlie Far",
            &line_gpx(11.8, 20.0),
            "public",
            "easy",
        )
        .await;
        upload_route(
            &app,
            &session,
            "Delta Unlisted",
            &line_gpx(10.009, 20.001),
            "unlisted",
            "easy",
        )
        .await;
        (app, dir)
    }

    async fn explore_body(app: &Router, query: &str) -> String {
        let response = send(
            app,
            "GET",
            &format!("/explore?{query}"),
            Body::empty(),
            None,
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        body_text(response).await
    }

    #[tokio::test]
    async fn explore_near_lists_public_routes_within_radius_nearest_first() {
        let (app, _dir) = nearby_fixture().await;
        let body = explore_body(&app, "near=10.00,20.00").await;
        let (alpha, bravo) = (
            body.find("Alpha Near").unwrap(),
            body.find("Bravo Middle").unwrap(),
        );
        assert!(alpha < bravo);
        assert!(!body.contains("Charlie Far"));
        assert!(!body.contains("Delta Unlisted"));
        assert!(body.contains("km away"));
    }

    #[tokio::test]
    async fn explore_near_keeps_filters() {
        let (app, _dir) = nearby_fixture().await;
        let body = explore_body(&app, "near=10.00,20.00&difficulty=hard").await;
        assert!(body.contains("Bravo Middle"));
        assert!(!body.contains("Alpha Near"));
    }

    #[tokio::test]
    async fn explore_ignores_invalid_near() {
        let (app, _dir) = nearby_fixture().await;
        let body = explore_body(&app, "near=999,0").await;
        for title in ["Alpha Near", "Bravo Middle", "Charlie Far"] {
            assert!(body.contains(title), "{title}");
        }
        assert!(!body.contains("km away"));
    }

    #[tokio::test]
    async fn explore_other_sort_ignores_near() {
        let (app, _dir) = nearby_fixture().await;
        let body = explore_body(&app, "near=10.00,20.00&sort=newest").await;
        for title in ["Alpha Near", "Bravo Middle", "Charlie Far"] {
            assert!(body.contains(title), "{title}");
        }
        assert!(!body.contains("km away"));
        assert!(body.contains("name=\"near\" value=\"10,20\""));
        assert!(body.contains("<option value=\"newest\" selected>"));
    }
}
