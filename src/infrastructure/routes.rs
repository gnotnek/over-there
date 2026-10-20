use sqlx::{FromRow, SqlitePool};

#[derive(Clone, Debug, FromRow)]
pub struct RouteRecord {
    pub id: String,
    pub author_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub activity: String,
    pub cycling_type: String,
    pub region: String,
    pub country: String,
    pub difficulty: String,
    pub tags: String,
    pub visibility: String,
    pub gpx_key: String,
    pub public_gpx_key: Option<String>,
    pub hide_endpoints_m: f64,
    pub geometry_json: String,
    pub elevation_json: Option<String>,
    pub distance_m: f64,
    pub elevation_gain_m: Option<f64>,
    pub elevation_loss_m: Option<f64>,
    pub duration_s: Option<i64>,
    pub download_count: i64,
    pub cover_key: Option<String>,
    pub created_at: String,
}

pub async fn find_by_slug(
    pool: &SqlitePool,
    slug: &str,
) -> Result<Option<RouteRecord>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM routes WHERE slug=?")
        .bind(slug)
        .fetch_optional(pool)
        .await
}
