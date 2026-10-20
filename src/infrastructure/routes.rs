use crate::domain::nearby;
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
    pub plan_json: Option<String>,
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

/// Fills the public-geometry box for routes stored before boxes existed. Idempotent.
pub async fn backfill_route_boxes(pool: &SqlitePool) -> Result<u64, sqlx::Error> {
    let missing: Vec<(String, String)> =
        sqlx::query_as("SELECT id, geometry_json FROM routes WHERE min_lat IS NULL")
            .fetch_all(pool)
            .await?;
    let mut updated = 0;
    for (id, geometry) in missing {
        let Some(b) = nearby::bbox(&geometry) else {
            continue;
        };
        updated +=
            sqlx::query("UPDATE routes SET min_lat=?,max_lat=?,min_lon=?,max_lon=? WHERE id=?")
                .bind(b.min_lat)
                .bind(b.max_lat)
                .bind(b.min_lon)
                .bind(b.max_lon)
                .bind(id)
                .execute(pool)
                .await?
                .rows_affected();
    }
    Ok(updated)
}
