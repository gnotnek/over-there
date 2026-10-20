use chrono::{DateTime, FixedOffset};
use roxmltree::{Document, Node, ParsingOptions};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Point {
    pub lon: f64,
    pub lat: f64,
    pub ele: Option<f64>,
    #[serde(skip)]
    pub time: Option<DateTime<FixedOffset>>,
}

#[derive(Debug)]
pub struct RouteData {
    pub segments: Vec<Vec<Point>>,
    pub distance_m: f64,
    pub gain_m: Option<f64>,
    pub loss_m: Option<f64>,
    pub duration_s: Option<i64>,
}

pub fn parse(bytes: &[u8]) -> Result<RouteData, String> {
    if bytes.len() > 10 * 1024 * 1024 {
        return Err("GPX must be 10 MB or less".into());
    }
    let xml = std::str::from_utf8(bytes).map_err(|_| "GPX must be UTF-8 XML")?;
    let doc = Document::parse_with_options(
        xml,
        ParsingOptions {
            nodes_limit: 500_000,
            ..Default::default()
        },
    )
    .map_err(|_| "Invalid GPX XML (DTD and external entities are not accepted)")?;
    if doc.root_element().tag_name().name() != "gpx" {
        return Err("This is not a GPX file".into());
    }
    let mut segments = Vec::new();
    for trk in doc.descendants().filter(|n| n.has_tag_name("trk")) {
        for seg in trk.children().filter(|n| n.has_tag_name("trkseg")) {
            let points = parse_points(seg, "trkpt")?;
            if !points.is_empty() {
                segments.push(points);
            }
        }
    }
    for rte in doc.descendants().filter(|n| n.has_tag_name("rte")) {
        let points = parse_points(rte, "rtept")?;
        if !points.is_empty() {
            segments.push(points);
        }
    }
    let count: usize = segments.iter().map(Vec::len).sum();
    if count < 2 {
        return Err("GPX needs at least two track or route points".into());
    }
    if count > 100_000 {
        return Err("GPX has too many points (limit 100,000)".into());
    }
    let mut distance_m = 0.0;
    let mut gain = 0.0;
    let mut loss = 0.0;
    let mut elevation_points = 0;
    for seg in &segments {
        for pair in seg.windows(2) {
            distance_m += haversine(&pair[0], &pair[1]);
            if let (Some(a), Some(b)) = (pair[0].ele, pair[1].ele) {
                elevation_points += 1;
                let change = b - a;
                // A small deadband avoids counting meter-scale GPS noise as climbing.
                if change > 3.0 {
                    gain += change;
                }
                if change < -3.0 {
                    loss -= change;
                }
            }
        }
    }
    let duration_s = if segments.len() == 1 {
        let times: Option<Vec<_>> = segments[0].iter().map(|p| p.time).collect();
        times.and_then(|times| {
            if times.windows(2).all(|w| w[1] >= w[0]) {
                let secs = (times.last()?.to_owned() - times.first()?.to_owned()).num_seconds();
                (secs > 0).then_some(secs)
            } else {
                None
            }
        })
    } else {
        None
    };
    Ok(RouteData {
        segments,
        distance_m,
        gain_m: (elevation_points > 0).then_some(gain),
        loss_m: (elevation_points > 0).then_some(loss),
        duration_s,
    })
}

pub fn elevation_profile(segments: &[Vec<Point>]) -> Option<Vec<(f64, f64, f64, f64)>> {
    let mut distance_m = 0.0;
    let mut elevation = Vec::new();
    for seg in segments {
        for pair in seg.windows(2) {
            distance_m += haversine(&pair[0], &pair[1]);
            if let Some(ele) = pair[1].ele {
                elevation.push((distance_m, ele, pair[1].lon, pair[1].lat));
            }
        }
    }
    (!elevation.is_empty()).then_some(elevation)
}

fn parse_points(parent: Node<'_, '_>, tag: &str) -> Result<Vec<Point>, String> {
    parent
        .children()
        .filter(|n| n.has_tag_name(tag))
        .map(|node| {
            let lat: f64 = node
                .attribute("lat")
                .ok_or("Point has no latitude")?
                .parse()
                .map_err(|_| "Invalid latitude")?;
            let lon: f64 = node
                .attribute("lon")
                .ok_or("Point has no longitude")?
                .parse()
                .map_err(|_| "Invalid longitude")?;
            if !lat.is_finite()
                || !lon.is_finite()
                || !(-90.0..=90.0).contains(&lat)
                || !(-180.0..=180.0).contains(&lon)
            {
                return Err("Point is outside valid coordinates".into());
            }
            let ele = node
                .children()
                .find(|n| n.has_tag_name("ele"))
                .and_then(|n| n.text())
                .and_then(|x| x.parse::<f64>().ok())
                .filter(|x| x.is_finite());
            let time = node
                .children()
                .find(|n| n.has_tag_name("time"))
                .and_then(|n| n.text())
                .and_then(|x| DateTime::parse_from_rfc3339(x).ok());
            Ok(Point {
                lon,
                lat,
                ele,
                time,
            })
        })
        .collect()
}

pub fn haversine(a: &Point, b: &Point) -> f64 {
    let r = 6_371_000.0;
    let dlat = (b.lat - a.lat).to_radians();
    let dlon = (b.lon - a.lon).to_radians();
    let h = (dlat / 2.0).sin().powi(2)
        + a.lat.to_radians().cos() * b.lat.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * h.clamp(0.0, 1.0).sqrt().asin()
}

pub fn public_segments(segments: &[Vec<Point>], hide_m: f64) -> Result<Vec<Vec<Point>>, String> {
    if hide_m <= 0.0 {
        return Ok(segments.to_vec());
    }
    let first = segments
        .iter()
        .find_map(|s| s.first())
        .ok_or("Route has no points")?;
    let last = segments
        .iter()
        .rev()
        .find_map(|s| s.last())
        .ok_or("Route has no points")?;
    let filtered: Vec<Vec<Point>> = segments
        .iter()
        .map(|seg| {
            seg.iter()
                .filter(|p| haversine(first, p) >= hide_m && haversine(last, p) >= hide_m)
                .cloned()
                .collect()
        })
        .filter(|seg: &Vec<Point>| seg.len() >= 2)
        .collect();
    if filtered.is_empty() {
        return Err("Endpoint hiding removes the whole route; choose a smaller radius".into());
    }
    Ok(filtered)
}

pub fn geojson(segments: &[Vec<Point>]) -> String {
    let lines: Vec<Vec<[f64; 2]>> = segments
        .iter()
        .filter(|s| s.len() >= 2)
        .map(|s| {
            simplify(s, 0.00007)
                .iter()
                .map(|p| [p.lon, p.lat])
                .collect()
        })
        .collect();
    serde_json::json!({"type":"MultiLineString","coordinates":lines}).to_string()
}

fn simplify(points: &[Point], tolerance: f64) -> Vec<Point> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    let a = &points[0];
    let b = &points[points.len() - 1];
    let mut max = 0.0;
    let mut index = 0;
    for (i, p) in points.iter().enumerate().skip(1).take(points.len() - 2) {
        let dx = b.lon - a.lon;
        let dy = b.lat - a.lat;
        let t = if dx * dx + dy * dy > 0.0 {
            ((p.lon - a.lon) * dx + (p.lat - a.lat) * dy) / (dx * dx + dy * dy)
        } else {
            0.0
        }
        .clamp(0.0, 1.0);
        let dist = ((p.lon - a.lon - t * dx).powi(2) + (p.lat - a.lat - t * dy).powi(2)).sqrt();
        if dist > max {
            max = dist;
            index = i;
        }
    }
    if max <= tolerance {
        return vec![a.clone(), b.clone()];
    }
    let mut left = simplify(&points[..=index], tolerance);
    left.pop();
    left.extend(simplify(&points[index..], tolerance));
    left
}

pub fn redacted_gpx(segments: &[Vec<Point>]) -> Vec<u8> {
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?><gpx version=\"1.1\" creator=\"Over There\" xmlns=\"http://www.topografix.com/GPX/1/1\"><trk><name>Shared route</name>");
    for seg in segments {
        xml.push_str("<trkseg>");
        for p in seg {
            xml.push_str(&format!("<trkpt lat=\"{}\" lon=\"{}\">", p.lat, p.lon));
            if let Some(ele) = p.ele {
                xml.push_str(&format!("<ele>{ele}</ele>"));
            }
            // Times are left out: they would show when the owner rode, which hiding is meant to protect.
            xml.push_str("</trkpt>");
        }
        xml.push_str("</trkseg>");
    }
    xml.push_str("</trk></gpx>");
    xml.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn segments_do_not_join_and_missing_metrics_are_absent() {
        let xml = br#"<gpx><trk><trkseg><trkpt lat="0" lon="0"/><trkpt lat="0" lon="0.01"/></trkseg><trkseg><trkpt lat="20" lon="20"/><trkpt lat="20" lon="20.01"/></trkseg></trk></gpx>"#;
        let r = parse(xml).unwrap();
        assert!(r.distance_m > 2_000.0 && r.distance_m < 2_300.0);
        assert_eq!(r.gain_m, None);
        assert_eq!(r.duration_s, None);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&geojson(&r.segments)).unwrap()
                ["coordinates"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }
    #[test]
    fn rejects_dtd_and_bad_coordinates() {
        assert!(parse(br#"<!DOCTYPE gpx [<!ENTITY x "hi">]><gpx/>"#).is_err());
        assert!(parse(
            br#"<gpx><rte><rtept lat="91" lon="0"/><rtept lat="0" lon="0"/></rte></gpx>"#
        )
        .is_err());
    }
    #[test]
    fn hiding_keeps_original_unmodified() {
        let xml = br#"<gpx><rte><rtept lat="0" lon="0"/><rtept lat="0" lon="0.01"/><rtept lat="0" lon="0.02"/><rtept lat="0" lon="0.03"/><rtept lat="0" lon="0.04"/></rte></gpx>"#;
        let r = parse(xml).unwrap();
        let public = public_segments(&r.segments, 500.0).unwrap();
        assert_eq!(r.segments[0].len(), 5);
        assert_eq!(public[0].len(), 3);
    }
    #[test]
    fn hidden_endpoints_are_absent_from_public_elevation() {
        let xml = br#"<gpx><rte><rtept lat="0" lon="0"><ele>1</ele></rtept><rtept lat="0" lon="0.01"><ele>2</ele></rtept><rtept lat="0" lon="0.02"><ele>3</ele></rtept><rtept lat="0" lon="0.03"><ele>4</ele></rtept><rtept lat="0" lon="0.04"><ele>5</ele></rtept></rte></gpx>"#;
        let parsed = parse(xml).unwrap();
        let public = public_segments(&parsed.segments, 500.0).unwrap();
        let profile = elevation_profile(&public).unwrap();
        assert!(profile
            .iter()
            .all(|point| point.2 != 0.0 && point.2 != 0.04));
    }
    #[test]
    fn redacted_gpx_drops_times_and_keeps_elevation() {
        let xml = br#"<gpx><trk><trkseg><trkpt lat="0" lon="0"><ele>1</ele><time>2026-01-01T08:00:00Z</time></trkpt><trkpt lat="0" lon="0.01"><ele>2</ele><time>2026-01-01T08:05:00Z</time></trkpt></trkseg></trk></gpx>"#;
        let redacted = redacted_gpx(&parse(xml).unwrap().segments);
        let reparsed = parse(&redacted).unwrap();
        assert!(!String::from_utf8(redacted).unwrap().contains("<time>"));
        assert_eq!(reparsed.duration_s, None);
        assert_eq!(reparsed.segments[0][1].ele, Some(2.0));
    }
}
