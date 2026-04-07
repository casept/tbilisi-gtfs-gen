use argh::FromArgs;
use gtfs_generator::GtfsGenerator;
use gtfs_structures::{
    Agency, Calendar, DirectionType, RawStopTime, RawTranslation, RawTrip, Route, RouteType, Shape,
    Stop,
};
use log::*;
use rayon::prelude::*;

use rgb::RGB8;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tbilisi_gtfs_gen::*;

/// Generate a static GTFS feed for Tbilisi public transport.
#[derive(FromArgs)]
struct Args {
    /// path to write the output GTFS zip (default: gtfs.zip)
    #[argh(option, short = 'o', default = "String::from(\"gtfs.zip\")")]
    output: String,

    /// log level filter (e.g. trace, debug, info, warn, error; default: info)
    #[argh(option, default = "String::from(\"info\")")]
    log_level: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct TtcRoute {
    pub id: String,
    pub short_name: String,
    pub long_name: Option<String>,
    pub color: String,
    pub mode: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct TtcRouteDetail {
    pub patterns: Vec<TtcPattern>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct TtcPattern {
    pub pattern_suffix: String,
    pub direction_id: u8,
    pub headsign: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct TtcStop {
    pub id: String,
    pub code: Option<String>,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct TtcSchedule {
    weekday_schedules: Vec<TtcWeekdaySchedule>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct TtcWeekdaySchedule {
    stops: Vec<TtcScheduleStop>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct TtcScheduleStop {
    id: String,
    arrival_times: String,
}

fn parse_color(hex: &str) -> RGB8 {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
        RGB8::new(r, g, b)
    } else {
        warn!("Cannot parse color \"{hex}\", returning default");
        RGB8::new(255, 255, 255)
    }
}

use crate::{BASE_URL, RateLimiter, fetch_with_retry};

#[derive(Deserialize, Debug)]
struct TtcPolylineEntry {
    #[serde(rename = "encodedValue")]
    encoded_value: String,
}

type TtcPolylineResponse = HashMap<String, TtcPolylineEntry>;

/// Precision used by the TTC encoded polyline (standard Google polyline algorithm).
const POLYLINE_PRECISION: u32 = 5;

/// Decode an encoded polyline string into GTFS Shape points for the given `shape_id`.
///
/// In geo-types coordinates, `x` is longitude and `y` is latitude.
fn decode_shape(
    shape_id: &str,
    encoded: &str,
) -> Result<Vec<Shape>, polyline::errors::PolylineError> {
    let line_string = polyline::decode_polyline(encoded, POLYLINE_PRECISION)?;
    Ok(line_string
        .0
        .into_iter()
        .enumerate()
        .map(|(seq, coord)| Shape {
            id: shape_id.to_string(),
            latitude: coord.y,
            longitude: coord.x,
            sequence: seq,
            dist_traveled: None,
        })
        .collect())
}

/// Fetch and decode shapes for all patterns of a route in individual API calls.
///
/// Returns a map from `pattern_suffix` → `Vec<Shape>`. Patterns for which the
/// API call or polyline decoding fails are omitted with a warning.
pub fn fetch_shapes_for_route(
    route_id: &str,
    pattern_suffixes: &[String],
    rate_limiter: &RateLimiter,
) -> HashMap<String, Vec<Shape>> {
    let mut result = HashMap::new();
    for suffix in pattern_suffixes {
        let url = format!(
            "{}/v3/routes/{}/polylines?patternSuffixes={}",
            BASE_URL, route_id, suffix
        );
        let resp: TtcPolylineResponse = match fetch_with_retry(&url, rate_limiter).and_then(|r| {
            r.into_json()
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        }) {
            Ok(r) => r,
            Err(e) => {
                warn!("Failed to fetch polyline for route {route_id} pattern {suffix}: {e}");
                continue;
            }
        };
        for (resp_suffix, entry) in resp {
            let shape_id = format!("{}-{}", route_id, resp_suffix);
            match decode_shape(&shape_id, &entry.encoded_value) {
                Ok(shapes) => {
                    result.insert(resp_suffix, shapes);
                }
                Err(e) => {
                    warn!(
                        "Failed to decode polyline for route {route_id} pattern {resp_suffix}: {e}"
                    );
                }
            }
        }
    }
    result
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args: Args = argh::from_env();
    env_logger::Builder::new()
        .parse_filters(&args.log_level)
        .init();

    let generator = Arc::new(Mutex::new(GtfsGenerator::new()));
    let rate_limiter = Arc::new(RateLimiter::new());

    let agency_id = "TTC".to_string();
    {
        let mut g = generator.lock().unwrap();
        g.add_agency(Agency {
            id: Some(agency_id.clone()),
            name: "Tbilisi Transport Company".to_string(),
            url: "https://ttc.com.ge".to_string(),
            timezone: "Asia/Tbilisi".to_string(),
            lang: Some("ka".to_string()),
            fare_url: Some("https://ttc.com.ge/index.php/en/fares".to_string()),
            email: Some("info@metro.ge".to_string()),
            phone: Some("032 293 44 44".to_string()),
        })?;

        // Add a single service for all days
        // FIXME: Probably not true
        let service_id = "EVERYDAY".to_string();
        g.add_service(Calendar {
            id: service_id.clone(),
            monday: true,
            tuesday: true,
            wednesday: true,
            thursday: true,
            friday: true,
            saturday: true,
            sunday: true,
            start_date: chrono::Utc::now()
                .with_timezone(&chrono_tz::Asia::Tbilisi)
                .date_naive(),
            end_date: chrono::Utc::now()
                .with_timezone(&chrono_tz::Asia::Tbilisi)
                .date_naive()
                + chrono::Duration::days(28),
        })?;
    }

    info!("Fetching stops...");
    let ttc_stops_ka: Vec<TtcStop> =
        fetch_with_retry(&format!("{}/v2/stops?locale=ka", BASE_URL), &rate_limiter)?
            .into_json()?;
    let ttc_stops_en: Vec<TtcStop> =
        fetch_with_retry(&format!("{}/v2/stops?locale=en", BASE_URL), &rate_limiter)?
            .into_json()?;
    let en_stop_names: HashMap<String, String> =
        ttc_stops_en.into_iter().map(|s| (s.id, s.name)).collect();
    {
        let mut g = generator.lock().unwrap();
        for s in &ttc_stops_ka {
            g.add_stop(Stop {
                id: s.id.clone(),
                code: s.code.clone(),
                name: Some(s.name.clone()),
                latitude: Some(s.lat),
                longitude: Some(s.lon),
                ..Default::default()
            })?;
            // Georgian is primary; add ka translation so consumers that look up by language find it.
            g.add_translation(RawTranslation {
                table_name: "stops".to_string(),
                field_name: "stop_name".to_string(),
                language: "ka".to_string(),
                translation: s.name.clone(),
                record_id: Some(s.id.clone()),
                record_sub_id: None,
                field_value: None,
            })?;
            if let Some(en_name) = en_stop_names.get(&s.id) {
                g.add_translation(RawTranslation {
                    table_name: "stops".to_string(),
                    field_name: "stop_name".to_string(),
                    language: "en".to_string(),
                    translation: en_name.clone(),
                    record_id: Some(s.id.clone()),
                    record_sub_id: None,
                    field_value: None,
                })?;
            }
        }
    }

    info!("Fetching routes...");
    let ttc_routes_ka: Vec<TtcRoute> =
        fetch_with_retry(&format!("{}/v3/routes?locale=ka", BASE_URL), &rate_limiter)?
            .into_json()?;
    let ttc_routes_en: Vec<TtcRoute> =
        fetch_with_retry(&format!("{}/v3/routes?locale=en", BASE_URL), &rate_limiter)?
            .into_json()?;
    let en_routes: Arc<HashMap<String, TtcRoute>> = Arc::new(
        ttc_routes_en
            .into_iter()
            .map(|r| (r.id.clone(), r))
            .collect(),
    );

    let service_id = "EVERYDAY".to_string();

    ttc_routes_ka.par_iter().for_each(|r| {
        info!("Fetching route {}", r.id);
        let rate_limiter = Arc::clone(&rate_limiter);
        let en_routes = Arc::clone(&en_routes);
        let route_type = match r.mode.as_str() {
            "SUBWAY" => RouteType::Subway,
            "BUS" => RouteType::Bus,
            "GONDOLA" => RouteType::Gondola,
            _ => {
                warn!("Unknown mode \"{}\", treating as bus", r.mode);
                RouteType::Bus
            },
        };

        {
            let mut g = generator.lock().unwrap();
            g.add_route(Route {
                id: r.id.clone(),
                agency_id: Some(agency_id.clone()),
                short_name: Some(r.short_name.clone()),
                long_name: r.long_name.clone(),
                route_type,
                color: Some(parse_color(&r.color)),
                ..Default::default()
            })
            .ok();
            // Georgian is primary; add ka translation so consumers that look up by language find it.
            if let Some(ref ka_long_name) = r.long_name {
                g.add_translation(RawTranslation {
                    table_name: "routes".to_string(),
                    field_name: "route_long_name".to_string(),
                    language: "ka".to_string(),
                    translation: ka_long_name.clone(),
                    record_id: Some(r.id.clone()),
                    record_sub_id: None,
                    field_value: None,
                })
                .ok();
            }
            if let Some(en_route) = en_routes.get(&r.id) {
                if let Some(ref en_long_name) = en_route.long_name {
                    g.add_translation(RawTranslation {
                        table_name: "routes".to_string(),
                        field_name: "route_long_name".to_string(),
                        language: "en".to_string(),
                        translation: en_long_name.clone(),
                        record_id: Some(r.id.clone()),
                        record_sub_id: None,
                        field_value: None,
                    })
                    .ok();
                }
            }
        }

        // Fetch Georgian route details (primary) to get patterns and headsigns
        let detail_url = format!("{}/v3/routes/{}?locale=ka", BASE_URL, r.id);
        let detail: TtcRouteDetail = match fetch_with_retry(&detail_url, &rate_limiter)
            .and_then(|r| r.into_json().map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>))
        {
            Ok(d) => d,
            Err(e) => {
                warn!("Failed to get route detail from {detail_url}: {e:?}");
                return;
            },
        };

        // Fetch English route detail to get English headsigns for translation
        let en_detail_url = format!("{}/v3/routes/{}?locale=en", BASE_URL, r.id);
        let en_detail: Option<TtcRouteDetail> = match fetch_with_retry(&en_detail_url, &rate_limiter)
            .and_then(|r| r.into_json().map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>))
        {
            Ok(d) => Some(d),
            Err(e) => {
                warn!("Failed to get English route detail from {en_detail_url}: {e:?}");
                None
            },
        };

        // Build map: ka_headsign -> en_headsign (keyed by field_value which is the Georgian primary)
        let en_by_suffix: HashMap<String, String> = en_detail
            .map(|d| d.patterns.into_iter().map(|p| (p.pattern_suffix, p.headsign)).collect())
            .unwrap_or_default();
        {
            let mut g = generator.lock().unwrap();
            for pattern in &detail.patterns {
                // Always add a ka translation so consumers looking up by language find it
                g.add_translation(RawTranslation {
                    table_name: "trips".to_string(),
                    field_name: "trip_headsign".to_string(),
                    language: "ka".to_string(),
                    translation: pattern.headsign.clone(),
                    record_id: None,
                    record_sub_id: None,
                    field_value: Some(pattern.headsign.clone()),
                })
                .ok();
                if let Some(en_headsign) = en_by_suffix.get(&pattern.pattern_suffix) {
                    g.add_translation(RawTranslation {
                        table_name: "trips".to_string(),
                        field_name: "trip_headsign".to_string(),
                        language: "en".to_string(),
                        translation: en_headsign.clone(),
                        record_id: None,
                        record_sub_id: None,
                        field_value: Some(pattern.headsign.clone()),
                    })
                    .ok();
                }
            }
        }

        let pattern_suffixes: Vec<String> =
            detail.patterns.iter().map(|p| p.pattern_suffix.clone()).collect();
        let route_shapes = fetch_shapes_for_route(&r.id, &pattern_suffixes, &rate_limiter);
        {
            let mut g = generator.lock().unwrap();
            for shape_vec in route_shapes.values() {
                for shape in shape_vec {
                    g.add_shape(shape.clone()).ok();
                }
            }
        }

        for pattern in detail.patterns {
            let schedule_url = format!(
                "{}/v2/routes/{}/schedule?locale=en&patternSuffix={}",
                BASE_URL, r.id, pattern.pattern_suffix
            );
            let schedule: TtcSchedule = match fetch_with_retry(&schedule_url, &rate_limiter)
                .and_then(|r| r.into_json().map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>))
            {
                Ok(s) => s,
                Err(e) => {
                    warn!("Failed to retrieve schedule for pattern {pattern:?} from \"{schedule_url}\": {e:?}");
                    return;
                },
            };

            if let Some(ws) = schedule.weekday_schedules.first() {
                let num_trips = ws
                    .stops
                    .first()
                    .map(|s| s.arrival_times.split(',').count())
                    .unwrap_or(0);

                for trip_idx in 0..num_trips {
                    let trip_id = format!("{}-{}-{}", r.id, pattern.pattern_suffix, trip_idx);
                    let direction = if pattern.direction_id == 0 {
                        DirectionType::Outbound
                    } else {
                        DirectionType::Inbound
                    };

                    let mut last_time = None;
                    let mut stop_times = Vec::new();
                    let mut valid_trip = true;

                    for (stop_idx, stop) in ws.stops.iter().enumerate() {
                        let times: Vec<&str> = stop.arrival_times.split(',').collect();
                        if let Some(time_str) = times.get(trip_idx) {
                            if time_str.is_empty() {
                                warn!("Empty arrival time for trip_idx {trip_idx}, stop_idx {stop_idx}, stop {stop:?}");
                                continue;
                            }

                            let parts: Vec<&str> = time_str.split(':').collect();
                            if parts.len() == 2 {
                                let h: u32 = parts[0].parse().unwrap_or(0);
                                let m: u32 = parts[1].parse().unwrap_or(0);
                                let mut seconds = h * 3600 + m * 60;

                                // Handle times past midnight (24:xx, 25:xx etc)
                                if let Some(prev) = last_time
                                    && seconds < prev {
                                        // If time jumps back more than 12 hours, assume it's actually next day
                                        // But if it's just a small jump back (like 1-2 minutes), it's probably data error
                                        if prev - seconds > 12 * 3600 {
                                            seconds += 24 * 3600;
                                        } else {
                                            warn!("Trip invalid (time jumps backwards). time_str = {time_str}, h = {h}, m = {m}, seconds = {seconds}, seconds_prev = {prev}");
                                            valid_trip = false;
                                            break;
                                        }
                                    }

                                stop_times.push(RawStopTime {
                                    trip_id: trip_id.clone(),
                                    arrival_time: Some(seconds),
                                    departure_time: Some(seconds),
                                    stop_id: stop.id.clone(),
                                    stop_sequence: stop_idx as u32,
                                    ..Default::default()
                                });
                                last_time = Some(seconds);
                            }
                        }
                    }

                    if valid_trip && !stop_times.is_empty() {
                        // Only reference a shape_id if the shape was actually fetched successfully
                        let shape_id = route_shapes
                            .contains_key(&pattern.pattern_suffix)
                            .then(|| format!("{}-{}", r.id, pattern.pattern_suffix));
                        let mut g = generator.lock().unwrap();
                        g.add_trip(RawTrip {
                            id: trip_id.clone(),
                            route_id: r.id.clone(),
                            service_id: service_id.clone(),
                            trip_headsign: Some(pattern.headsign.clone()),
                            direction_id: Some(direction),
                            shape_id,
                            ..Default::default()
                        })
                        .ok();

                        for st in stop_times {
                            g.add_stop_time(st).ok();
                        }
                    }
                }
            }
        }
        info!("Processed route {}", r.id);
    });

    let g_final = Arc::try_unwrap(generator)
        .map_err(|_| "Arc unwrap failed")?
        .into_inner()?;
    g_final.write_to(&args.output)?;
    info!("Successfully generated GTFS feed to {}", args.output);

    Ok(())
}
