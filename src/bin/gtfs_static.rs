use gtfs_generator::GtfsGenerator;
use gtfs_structures::{
    Agency, Calendar, DirectionType, RawStopTime, RawTrip, Route, RouteType, Stop,
};
use log::*;
use rayon::prelude::*;
use reqwest::header::{HeaderMap, HeaderValue};
use rgb::RGB8;
use serde::Deserialize;
use std::sync::{Arc, Mutex};
use tbilisi_gtfs_gen::{API_KEY, BASE_URL};

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let mut headers = HeaderMap::new();
    headers.insert("X-api-key", HeaderValue::from_static(API_KEY));
    let client = reqwest::blocking::Client::builder()
        .default_headers(headers)
        .build()?;

    let generator = Arc::new(Mutex::new(GtfsGenerator::new()));

    let agency_id = "TTC".to_string();
    {
        let mut g = generator.lock().unwrap();
        g.add_agency(Agency {
            id: Some(agency_id.clone()),
            name: "Tbilisi Transport Company".to_string(),
            url: "https://ttc.com.ge".to_string(),
            timezone: "Asia/Tbilisi".to_string(),
            lang: Some("ka".to_string()),
            ..Default::default()
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
            start_date: chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            end_date: chrono::NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(),
        })?;
    }

    info!("Fetching stops...");
    let ttc_stops: Vec<TtcStop> = client
        .get(format!("{}/v2/stops?locale=en", BASE_URL))
        .send()?
        .json()?;
    {
        let mut g = generator.lock().unwrap();
        for s in &ttc_stops {
            g.add_stop(Stop {
                id: s.id.clone(),
                code: s.code.clone(),
                name: Some(s.name.clone()),
                latitude: Some(s.lat),
                longitude: Some(s.lon),
                ..Default::default()
            })?;
        }
    }

    info!("Fetching routes...");
    let ttc_routes: Vec<TtcRoute> = client
        .get(format!("{}/v3/routes?locale=en", BASE_URL))
        .send()?
        .json()?;

    let service_id = "EVERYDAY".to_string();

    ttc_routes.par_iter().for_each(|r| {
        let mut headers = HeaderMap::new();
        headers.insert("X-api-key", HeaderValue::from_static(API_KEY));
        let client = reqwest::blocking::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap();

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
        }

        // Fetch route details to get patterns
        let detail_url = format!("{}/v3/routes/{}?locale=en", BASE_URL, r.id);
        let detail_res = client
            .get(detail_url.as_str())
            .send();
        let detail: TtcRouteDetail = match detail_res {
            Ok(resp) => match resp.json() {
                Ok(d) => d,
                Err(e) => {
                    warn!("Failed to parse route detail: {e:?}");
                    return;
                },
            },
            Err(e) => {
                    warn!("Failed to get route detail from {detail_url}: {e:?}");
                    return;
            },
        };

        for pattern in detail.patterns {
            let schedule_url = format!(
                "{}/v2/routes/{}/schedule?locale=en&patternSuffix={}",
                BASE_URL, r.id, pattern.pattern_suffix
            );
            let schedule: TtcSchedule = match client.get(schedule_url.clone()).send().and_then(|r| r.json())
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
                                if let Some(prev) = last_time {
                                    if seconds < prev {
                                        // If time jumps back more than 12 hours, assume it's actually next day
                                        // But if it's just a small jump back (like 1-2 minutes), it's probably data error
                                        if prev - seconds > 12 * 3600 {
                                            seconds += 24 * 3600;
                                        } else {
                                            // Real data error (non-monotonous time)
                                            warn!("Trip invalid (non-monotonous time)");
                                            valid_trip = false;
                                            break;
                                        }
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
                        let mut g = generator.lock().unwrap();
                        g.add_trip(RawTrip {
                            id: trip_id.clone(),
                            route_id: r.id.clone(),
                            service_id: service_id.clone(),
                            trip_headsign: Some(pattern.headsign.clone()),
                            direction_id: Some(direction),
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
    g_final.write_to("gtfs.zip")?;
    info!("Successfully generated GTFS feed to gtfs.zip");

    Ok(())
}
