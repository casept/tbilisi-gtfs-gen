use gtfs_realtime::*;
use gtfs_structures::Gtfs;
use log::*;
use prost::Message;

use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use tbilisi_gtfs_gen::*;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct TtcVehicle {
    vehicle_id: String,
    lat: f64,
    lon: f64,
    heading: Option<f64>,
    next_stop_id: Option<String>,
}

type TtcPositionsResponse = HashMap<String, Vec<TtcVehicle>>;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    info!("Loading static GTFS from gtfs.zip...");
    let gtfs = Gtfs::from_path("gtfs.zip")?;

    // Map: route_id -> pattern_suffix -> Vec<trip_id>
    let mut route_patterns: HashMap<String, HashMap<String, Vec<String>>> = HashMap::new();
    for trip in gtfs.trips.values() {
        if let Some((_, suffix_and_idx)) = trip.id.split_once('-')
            && let Some((suffix, _)) = suffix_and_idx.split_once('-')
        {
            route_patterns
                .entry(trip.route_id.clone())
                .or_default()
                .entry(suffix.to_string())
                .or_default()
                .push(trip.id.clone());
        }
    }

    let mut entities = Vec::new();
    let now = chrono::Utc::now();
    let seconds_since_midnight = (now.naive_utc().time()
        - chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap())
    .num_seconds() as u32;

    let rate_limiter = Arc::new(RateLimiter::new());
    for (route_id, patterns) in &route_patterns {
        let suffixes: Vec<String> = patterns.keys().cloned().collect();
        let suffix_str = suffixes.join(",");
        let url = format!(
            "{}/v3/routes/{}/positions?patternSuffixes={}",
            BASE_URL, route_id, suffix_str
        );

        debug!("Requesting positions for route {route_id}, patterns {route_patterns:?} from {url}");
        let resp: TtcPositionsResponse = match fetch_with_retry(&url, &rate_limiter) {
            Ok(r) => match r.into_json() {
                Ok(data) => data,
                Err(e) => {
                    warn!("Failed to decode JSON for route {route_id} from {url}: {e:?}");
                    continue;
                }
            },
            Err(e) => {
                warn!("Failed to fetch positions for route {route_id} from {url}: {e:?}");
                continue;
            }
        };

        for (suffix, vehicles) in resp {
            let possible_trip_ids = match patterns.get(&suffix) {
                Some(ids) => ids,
                None => {
                    warn!("Unknown pattern suffix \"{suffix}\" for route {route_id}");
                    continue;
                }
            };

            for vehicle in vehicles {
                // Try to find the best trip_id
                let mut best_trip_id = None;

                if let Some(ref next_stop_id) = vehicle.next_stop_id {
                    let mut min_diff = i64::MAX;

                    for trip_id in possible_trip_ids {
                        let trip = &gtfs.trips[trip_id];
                        // Find stop time for next_stop_id
                        if let Some(st) = trip
                            .stop_times
                            .iter()
                            .find(|st| st.stop.id == *next_stop_id)
                            && let Some(arrival) = st.arrival_time
                        {
                            let diff = (arrival as i64 - seconds_since_midnight as i64).abs();
                            if diff < min_diff {
                                min_diff = diff;
                                best_trip_id = Some(trip_id.clone());
                            }
                        }
                    }
                }

                debug!(
                    "Vehicle {}: pos=({:.6}, {:.6}), next_stop={:?}, matched_trip={:?}",
                    vehicle.vehicle_id,
                    vehicle.lat,
                    vehicle.lon,
                    vehicle.next_stop_id,
                    best_trip_id
                );

                // If we couldn't match by next_stop_id, or it was missing, maybe just use the first trip that's active?
                // For now, if we have no next_stop_id, we can't do much better than just providing route_id.

                let trip_descriptor = TripDescriptor {
                    trip_id: best_trip_id,
                    route_id: Some(route_id.clone()),
                    ..Default::default()
                };

                let vehicle_descriptor = VehicleDescriptor {
                    id: Some(vehicle.vehicle_id.clone()),
                    ..Default::default()
                };

                let position = Position {
                    latitude: vehicle.lat as f32,
                    longitude: vehicle.lon as f32,
                    bearing: vehicle.heading.map(|h| h as f32),
                    ..Default::default()
                };

                let vehicle_position = VehiclePosition {
                    trip: Some(trip_descriptor),
                    vehicle: Some(vehicle_descriptor),
                    position: Some(position),
                    stop_id: vehicle.next_stop_id,
                    timestamp: Some(now.timestamp() as u64),
                    ..Default::default()
                };

                entities.push(FeedEntity {
                    id: vehicle.vehicle_id.clone(),
                    vehicle: Some(vehicle_position),
                    ..Default::default()
                });
            }
        }
        info!("Processed positions for route {}", route_id);
    }

    let header = FeedHeader {
        gtfs_realtime_version: "2.0".to_string(),
        incrementality: Some(0), // FULL_DATASET
        timestamp: Some(now.timestamp() as u64),
        ..Default::default()
    };

    let num_entities = entities.len();
    let message = FeedMessage {
        header,
        entity: entities,
    };

    let mut buf = Vec::new();
    message.encode(&mut buf)?;

    let mut file = File::create("gtfs-rt.pb")?;
    file.write_all(&buf)?;

    info!(
        "Successfully wrote GTFS-RT feed with {} entities to gtfs-rt.pb",
        num_entities
    );

    Ok(())
}
