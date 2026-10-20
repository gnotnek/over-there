use crate::domain::gpx::{self, Point};
use serde_json::Value;

pub const RADIUS_M: f64 = 50_000.0;
pub const LIMIT: usize = 20;
const METRES_PER_DEGREE: f64 = 111_320.0;

/// A rider location, always rounded to about 1 km before use.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Near {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BBox {
    pub min_lat: f64,
    pub max_lat: f64,
    pub min_lon: f64,
    pub max_lon: f64,
}

pub fn parse_near(raw: &str) -> Option<Near> {
    let mut parts = raw.split(',').map(|p| p.trim().parse::<f64>().ok());
    let (lat, lon) = (parts.next()??, parts.next()??);
    if parts.next().is_some() || !lat.is_finite() || !lon.is_finite() {
        return None;
    }
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return None;
    }
    let round = |v: f64| (v * 100.0).round() / 100.0;
    Some(Near {
        lat: round(lat),
        lon: round(lon),
    })
}

/// Every `[lon, lat]` of a stored MultiLineString, line by line.
fn lines(geometry_json: &str) -> Vec<Vec<[f64; 2]>> {
    let value: Value = serde_json::from_str(geometry_json).unwrap_or(Value::Null);
    let Some(lines) = value["coordinates"].as_array() else {
        return Vec::new();
    };
    lines
        .iter()
        .filter_map(Value::as_array)
        .map(|line| {
            line.iter()
                .filter_map(|p| Some([p.get(0)?.as_f64()?, p.get(1)?.as_f64()?]))
                .collect()
        })
        .collect()
}

pub fn bbox(geometry_json: &str) -> Option<BBox> {
    let points: Vec<[f64; 2]> = lines(geometry_json).into_iter().flatten().collect();
    let first = points.first()?;
    let start = BBox {
        min_lat: first[1],
        max_lat: first[1],
        min_lon: first[0],
        max_lon: first[0],
    };
    Some(points.iter().fold(start, |b, [lon, lat]| BBox {
        min_lat: b.min_lat.min(*lat),
        max_lat: b.max_lat.max(*lat),
        min_lon: b.min_lon.min(*lon),
        max_lon: b.max_lon.max(*lon),
    }))
}

pub fn search_box(near: Near, radius_m: f64) -> BBox {
    let dlat = radius_m / METRES_PER_DEGREE;
    let (min_lat, max_lat) = ((near.lat - dlat).max(-90.0), (near.lat + dlat).min(90.0));
    let dlon = radius_m / (METRES_PER_DEGREE * near.lat.to_radians().cos());
    let (min_lon, max_lon) = (near.lon - dlon, near.lon + dlon);
    // shortcut: near a pole or the antimeridian the whole longitude band is searched; fine at this scale.
    if min_lat <= -90.0
        || max_lat >= 90.0
        || min_lon < -180.0
        || max_lon > 180.0
        || !dlon.is_finite()
    {
        return BBox {
            min_lat,
            max_lat,
            min_lon: -180.0,
            max_lon: 180.0,
        };
    }
    BBox {
        min_lat,
        max_lat,
        min_lon,
        max_lon,
    }
}

pub fn distance_m(geometry_json: &str, near: Near) -> Option<f64> {
    let rider = Point {
        lon: near.lon,
        lat: near.lat,
        ele: None,
        time: None,
    };
    lines(geometry_json)
        .into_iter()
        .flatten()
        .map(|[lon, lat]| {
            gpx::haversine(
                &rider,
                &Point {
                    lon,
                    lat,
                    ele: None,
                    time: None,
                },
            )
        })
        .min_by(f64::total_cmp)
}

pub fn first_point(geometry_json: &str) -> Option<[f64; 2]> {
    lines(geometry_json).into_iter().flatten().next()
}

/// Items within `RADIUS_M` of the rider, nearest first, at most `LIMIT`.
pub fn rank<T>(items: Vec<T>, geometry: impl Fn(&T) -> &str, near: Near) -> Vec<(T, f64)> {
    let mut ranked: Vec<(T, f64)> = items
        .into_iter()
        .filter_map(|item| {
            let d = distance_m(geometry(&item), near)?;
            (d <= RADIUS_M).then_some((item, d))
        })
        .collect();
    ranked.sort_by(|a, b| a.1.total_cmp(&b.1));
    ranked.truncate(LIMIT);
    ranked
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(lat: f64, lon: f64) -> Point {
        Point {
            lon,
            lat,
            ele: None,
            time: None,
        }
    }

    fn line(points: &[[f64; 2]]) -> String {
        serde_json::json!({"type":"MultiLineString","coordinates":[points]}).to_string()
    }

    #[test]
    fn parse_near_accepts_trims_and_rounds() {
        assert_eq!(
            parse_near(" 38.70712 , -9.13551 "),
            Some(Near {
                lat: 38.71,
                lon: -9.14
            })
        );
        assert!(parse_near("-90,180").is_some());
    }

    #[test]
    fn parse_near_rejects_bad_input() {
        for raw in ["", "1", "1,2,3", "91,0", "0,181", "NaN,1", "inf,0", "a,b"] {
            assert_eq!(parse_near(raw), None, "{raw:?}");
        }
    }

    #[test]
    fn bbox_covers_every_segment() {
        let geometry =
            r#"{"type":"MultiLineString","coordinates":[[[0,0],[1,1]],[[-2,3],[0.5,-1]]]}"#;
        assert_eq!(
            bbox(geometry),
            Some(BBox {
                min_lat: -1.0,
                max_lat: 3.0,
                min_lon: -2.0,
                max_lon: 1.0
            })
        );
        assert_eq!(bbox("{}"), None);
        assert_eq!(bbox(r#"{"type":"MultiLineString","coordinates":[]}"#), None);
    }

    #[test]
    fn search_box_contains_points_at_the_radius() {
        let near = Near {
            lat: 38.7,
            lon: -9.1,
        };
        let b = search_box(near, RADIUS_M);
        let dlat = 49_900.0 / 111_320.0;
        let dlon = 49_900.0 / (111_320.0 * near.lat.to_radians().cos());
        for (lat, lon) in [
            (near.lat + dlat, near.lon),
            (near.lat - dlat, near.lon),
            (near.lat, near.lon + dlon),
            (near.lat, near.lon - dlon),
        ] {
            assert!(gpx::haversine(&point(near.lat, near.lon), &point(lat, lon)) < RADIUS_M);
            assert!(lat >= b.min_lat && lat <= b.max_lat && lon >= b.min_lon && lon <= b.max_lon);
        }
    }

    #[test]
    fn search_box_widens_at_poles_and_antimeridian() {
        for near in [
            Near {
                lat: 89.9,
                lon: 0.0,
            },
            Near {
                lat: 0.0,
                lon: 179.9,
            },
        ] {
            let b = search_box(near, RADIUS_M);
            assert_eq!((b.min_lon, b.max_lon), (-180.0, 180.0));
        }
    }

    #[test]
    fn distance_ignores_hidden_endpoints() {
        let rider = Near {
            lat: 10.0,
            lon: 20.0,
        };
        let segment: Vec<Point> = (0..=40)
            .map(|i| point(10.0 + i as f64 * 0.001, 20.0))
            .collect();
        let full = gpx::geojson(&[segment.clone()]);
        let public = gpx::geojson(&gpx::public_segments(&[segment], 1000.0).unwrap());
        assert!(distance_m(&full, rider).unwrap() < 1.0);
        assert!(distance_m(&public, rider).unwrap() >= 900.0);
    }

    #[test]
    fn rank_filters_sorts_and_caps() {
        let near = Near {
            lat: 10.0,
            lon: 20.0,
        };
        let mut items: Vec<String> = (1..=30)
            .rev()
            .map(|km| line(&[[20.0, 10.0 + km as f64 * 1000.0 / 111_195.0]]))
            .collect();
        items.extend((0..3).map(|_| line(&[[20.0, 10.0 + 60_000.0 / 111_195.0]])));
        let ranked = rank(items, |g| g.as_str(), near);
        assert_eq!(ranked.len(), LIMIT);
        assert!(ranked.windows(2).all(|w| w[0].1 <= w[1].1));
        assert!(ranked.iter().all(|(_, d)| *d <= RADIUS_M));
        assert!(ranked[0].1 < 1_100.0);
    }

    #[test]
    fn first_point_is_start_of_first_line() {
        assert_eq!(
            first_point(&line(&[[20.0, 10.0], [21.0, 11.0]])),
            Some([20.0, 10.0])
        );
        assert_eq!(first_point("{}"), None);
    }
}
