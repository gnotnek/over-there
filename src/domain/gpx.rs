use chrono::{DateTime, FixedOffset};
use roxmltree::{Document, Node, ParsingOptions};
use serde::{Deserialize, Serialize};

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
        // Climbing is measured from a reference height that moves only after a change of
        // more than 3 m, so GPS noise is ignored but a climb made of small steps still counts.
        let mut reference = seg.iter().find_map(|p| p.ele);
        for pair in seg.windows(2) {
            distance_m += haversine(&pair[0], &pair[1]);
            if pair[0].ele.is_some() && pair[1].ele.is_some() {
                elevation_points += 1;
            }
            if let (Some(r), Some(e)) = (reference, pair[1].ele) {
                if e - r > 3.0 {
                    gain += e - r;
                    reference = Some(e);
                } else if r - e > 3.0 {
                    loss += r - e;
                    reference = Some(e);
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
    track_gpx("Shared route", segments)
}

/// GPX for a line drawn on the map, stored as the route's original file.
pub fn line_gpx(line: &[Point]) -> Vec<u8> {
    track_gpx("Planned route", &[line.to_vec()])
}

fn track_gpx(name: &str, segments: &[Vec<Point>]) -> Vec<u8> {
    let mut xml = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><gpx version=\"1.1\" creator=\"Over There\" xmlns=\"http://www.topografix.com/GPX/1/1\"><trk><name>{name}</name>");
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

/// The points a rider placed when drawing a route, so the line can be edited later.
/// `at[k]` is the index in the line where `points[k]` lies; `straight[k]` marks the leg after it.
#[derive(Debug, Deserialize, Serialize)]
pub struct Plan {
    pub points: Vec<[f64; 2]>,
    pub at: Vec<usize>,
    pub straight: Vec<bool>,
}

/// Checks a drawn line (`[[lon, lat, ele?], ...]`) and its plan, and returns the GPX and the plan as JSON.
pub fn drawn_route(line_json: &[u8], plan_json: &[u8]) -> Result<(Vec<u8>, String), String> {
    const BROKEN: &str = "The drawn route is incomplete; draw it again";
    let raw: Vec<Vec<f64>> = serde_json::from_slice(line_json).map_err(|_| BROKEN)?;
    let plan: Plan = serde_json::from_slice(plan_json).map_err(|_| BROKEN)?;
    if raw.len() < 2 || raw.len() > 100_000 {
        return Err("A drawn route needs 2 to 100,000 line points".into());
    }
    let line = raw
        .iter()
        .map(|c| {
            let (lon, lat, ele) = match c[..] {
                [lon, lat] => (lon, lat, None),
                [lon, lat, ele] => (lon, lat, Some(ele).filter(|e| e.is_finite())),
                _ => return Err(BROKEN.to_owned()),
            };
            if !lat.is_finite()
                || !lon.is_finite()
                || !(-90.0..=90.0).contains(&lat)
                || !(-180.0..=180.0).contains(&lon)
            {
                return Err("Point is outside valid coordinates".into());
            }
            Ok(Point {
                lon,
                lat,
                ele,
                time: None,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let n = plan.points.len();
    if !(2..=100).contains(&n) {
        return Err("A drawn route needs 2 to 100 points".into());
    }
    let consistent = plan.at.len() == n
        && plan.straight.len() == n - 1
        && plan.at[0] == 0
        && plan.at[n - 1] == line.len() - 1
        && plan.at.windows(2).all(|w| w[0] < w[1])
        && plan
            .points
            .iter()
            .zip(&plan.at)
            .all(|(p, &i)| p[0] == line[i].lon && p[1] == line[i].lat);
    if !consistent {
        return Err(BROKEN.into());
    }
    Ok((line_gpx(&line), serde_json::to_string(&plan).unwrap()))
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
    fn rte(eles: impl Iterator<Item = f64>) -> String {
        let points: String = eles
            .enumerate()
            .map(|(i, e)| {
                format!(
                    "<rtept lat=\"0\" lon=\"{}\"><ele>{e}</ele></rtept>",
                    i as f64 * 0.0005
                )
            })
            .collect();
        format!("<gpx><rte>{points}</rte></gpx>")
    }
    #[test]
    fn gentle_climb_counts_in_full() {
        let r = parse(rte((0..=40).map(f64::from)).as_bytes()).unwrap();
        assert_eq!(r.gain_m, Some(40.0));
        assert_eq!(r.loss_m, Some(0.0));
    }
    #[test]
    fn small_noise_is_not_climbing() {
        let r =
            parse(rte((0..20).map(|i| if i % 2 == 0 { 10.0 } else { 12.0 })).as_bytes()).unwrap();
        assert_eq!(r.gain_m, Some(0.0));
    }
    const LINE: &str = "[[121,25,5],[121,25.01,9],[121,25.02,30]]";
    const PLAN: &str = r#"{"points":[[121,25],[121,25.02]],"at":[0,2],"straight":[false]}"#;
    #[test]
    fn line_gpx_round_trips() {
        let point = |lat: f64, ele: Option<f64>| Point {
            lon: 121.0,
            lat,
            ele,
            time: None,
        };
        let line = vec![
            point(25.0, Some(5.0)),
            point(25.01, None),
            point(25.02, Some(30.0)),
        ];
        let r = parse(&line_gpx(&line)).unwrap();
        assert_eq!(r.segments.len(), 1);
        for (a, b) in r.segments[0].iter().zip(&line) {
            assert_eq!((a.lat, a.lon, a.ele), (b.lat, b.lon, b.ele));
        }
        let expected = haversine(&line[0], &line[1]) + haversine(&line[1], &line[2]);
        assert!((r.distance_m - expected).abs() < 1e-6);
    }
    #[test]
    fn drawn_route_accepts_a_valid_plan() {
        let (gpx, plan) = drawn_route(LINE.as_bytes(), PLAN.as_bytes()).unwrap();
        assert_eq!(parse(&gpx).unwrap().segments[0].len(), 3);
        assert_eq!(serde_json::from_str::<Plan>(&plan).unwrap().at, vec![0, 2]);
    }
    #[test]
    fn drawn_route_rejects_broken_plans() {
        let many: Vec<String> = (0..101)
            .map(|i| format!("[121,{}]", 25.0 + i as f64 * 0.001))
            .collect();
        let many_line = format!("[{}]", many.join(","));
        let many_plan = format!(
            r#"{{"points":[{}],"at":[{}],"straight":[{}]}}"#,
            many.join(","),
            (0..101)
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(","),
            vec!["false"; 100].join(",")
        );
        let broken = [
            (LINE, r#"{"points":[[121,25]],"at":[0],"straight":[]}"#),
            (many_line.as_str(), many_plan.as_str()),
            (
                LINE,
                r#"{"points":[[121,25.01],[121,25.02]],"at":[1,2],"straight":[false]}"#,
            ),
            (
                LINE,
                r#"{"points":[[121,25],[121,25.01]],"at":[0,1],"straight":[false]}"#,
            ),
            (
                LINE,
                r#"{"points":[[121,25],[121,25],[121,25.02]],"at":[0,0,2],"straight":[false,false]}"#,
            ),
            (
                LINE,
                r#"{"points":[[121,25],[121,25.02]],"at":[0,2],"straight":[]}"#,
            ),
            (
                LINE,
                r#"{"points":[[121,25],[121,25.03]],"at":[0,2],"straight":[false]}"#,
            ),
            (
                "[[121,91],[121,25.02]]",
                r#"{"points":[[121,91],[121,25.02]],"at":[0,1],"straight":[false]}"#,
            ),
            ("[[121],[121,25.02]]", PLAN),
            ("[[121,25,1,2],[121,25.02]]", PLAN),
            ("not json", PLAN),
            (LINE, "not json"),
        ];
        for (line, plan) in broken {
            assert!(
                drawn_route(line.as_bytes(), plan.as_bytes()).is_err(),
                "{line} {plan}"
            );
        }
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
