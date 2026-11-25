//! Local-network HTTP and WebSocket host for browser-based Riftbound sessions.

use std::{
    collections::BTreeMap,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use axum::{
    Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    response::Html,
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, broadcast};

const PORT: u16 = 7878;
const WORLD_WIDTH: f32 = 2200.0;
const WORLD_HEIGHT: f32 = 1500.0;
const PICKUP_RANGE: f32 = 48.0;
const SHARD_POSITIONS: [(f32, f32); 5] = [
    (380.0, 330.0),
    (1780.0, 360.0),
    (430.0, 1190.0),
    (1780.0, 1150.0),
    (1100.0, 250.0),
];
const EXTRACTION_POSITION: (f32, f32) = (1100.0, 750.0);
const PLAYER_SPEED: f32 = 230.0;
const MAX_INPUT_DELTA: f32 = 0.1;
const MAX_PLAYERS: usize = 16;
const MIN_MOVE_INTERVAL: Duration = Duration::from_millis(16);
const PLAYER_RADIUS: f32 = 15.0;
const VEHICLE_RADIUS: f32 = 24.0;
const MAX_VEHICLE_COLLISION_STEP: f32 = 8.0;
const WORLD_OBSTACLES: [(f32, f32, f32, f32); 11] = [
    (650.0, 170.0, 42.0, 350.0),
    (650.0, 520.0, 320.0, 42.0),
    (1450.0, 180.0, 42.0, 340.0),
    (1450.0, 520.0, 330.0, 42.0),
    (250.0, 700.0, 370.0, 42.0),
    (1580.0, 820.0, 370.0, 42.0),
    (780.0, 980.0, 42.0, 300.0),
    (1380.0, 980.0, 42.0, 300.0),
    (960.0, 580.0, 280.0, 36.0),
    (960.0, 900.0, 280.0, 36.0),
    (1008.0, 620.0, 36.0, 280.0),
];

#[derive(Clone)]
struct HostState {
    players: Arc<Mutex<BTreeMap<u64, PlayerState>>>,
    updates: broadcast::Sender<String>,
    next_id: Arc<AtomicU64>,
    shards: Arc<Mutex<[bool; 5]>>,
    extracted: Arc<Mutex<bool>>,
    enemies: Arc<Mutex<Vec<EnemyState>>>,
    vehicle: Arc<Mutex<SharedVehicleState>>,
    vehicle_driver: Arc<Mutex<Option<u64>>>,
}

#[derive(Clone, Serialize)]
struct PlayerState {
    id: u64,
    x: f32,
    y: f32,
    face: f32,
}

#[derive(Clone, Serialize)]
struct SharedVehicleState {
    x: f32,
    y: f32,
    heading: f32,
    speed: f32,
}

#[derive(Clone, Serialize)]
struct EnemyState {
    id: usize,
    x: f32,
    y: f32,
    hp: u8,
    stun: f32,
}

#[derive(Deserialize)]
struct ClientMessage {
    kind: String,
    dx: Option<f32>,
    dy: Option<f32>,
    face: Option<f32>,
    shard: Option<usize>,
    enemy: Option<usize>,
    throttle: Option<f32>,
    brake: Option<f32>,
    steering: Option<f32>,
}

#[derive(Serialize)]
struct Snapshot<'a> {
    kind: &'static str,
    players: &'a [PlayerState],
    shards: &'a [bool; 5],
    extracted: bool,
    enemies: &'a [EnemyState],
    vehicle: &'a SharedVehicleState,
    vehicle_driver: Option<u64>,
}

/// Starts the LAN web host and multiplayer relay on all local network interfaces.
///
/// # Errors
/// Returns an error if the operating system cannot create the server thread.
pub fn spawn() -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("riftbound-lan-server".to_owned())
        .spawn(|| {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    log::error!("Could not initialize LAN server runtime: {error}");
                    return;
                }
            };
            runtime.block_on(serve());
        })
        .map(|_| ())
}

/// Starts the HTTP and WebSocket server until the host process exits.
pub async fn serve() {
    let (updates, _) = broadcast::channel(128);
    let state = HostState {
        players: Arc::new(Mutex::new(BTreeMap::new())),
        updates,
        next_id: Arc::new(AtomicU64::new(1)),
        shards: Arc::new(Mutex::new([false; 5])),
        extracted: Arc::new(Mutex::new(false)),
        vehicle: Arc::new(Mutex::new(SharedVehicleState {
            x: WORLD_WIDTH * 0.5 + 90.0,
            y: WORLD_HEIGHT * 0.5,
            heading: 0.0,
            speed: 0.0,
        })),
        vehicle_driver: Arc::new(Mutex::new(None)),
        enemies: Arc::new(Mutex::new(vec![
            EnemyState {
                id: 0,
                x: 700.0,
                y: 500.0,
                hp: 2,
                stun: 0.0,
            },
            EnemyState {
                id: 1,
                x: 1500.0,
                y: 700.0,
                hp: 2,
                stun: 0.0,
            },
            EnemyState {
                id: 2,
                x: 900.0,
                y: 1080.0,
                hp: 2,
                stun: 0.0,
            },
            EnemyState {
                id: 3,
                x: 1840.0,
                y: 930.0,
                hp: 2,
                stun: 0.0,
            },
        ])),
    };
    let snapshot_state = state.clone();
    tokio::spawn(async move {
        let mut cadence = tokio::time::interval(Duration::from_millis(50));
        loop {
            cadence.tick().await;
            publish_snapshot(&snapshot_state).await;
        }
    });
    let app = Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/ws", get(websocket))
        .with_state(state);
    let address = SocketAddr::from(([0, 0, 0, 0], PORT));
    let listener = match tokio::net::TcpListener::bind(address).await {
        Ok(listener) => listener,
        Err(error) => {
            log::error!("Could not start Riftbound LAN host on port {PORT}: {error}");
            return;
        }
    };
    let lan_ip = discover_lan_ip().unwrap_or_else(|| "PC-LAN-IP".to_owned());
    log::info!("Riftbound host is ready. Play on this PC: http://127.0.0.1:{PORT}");
    log::info!("Join from other devices on the same Wi-Fi: http://{lan_ip}:{PORT}");
    log::info!("The PC browser and mobile browsers join the same hosted session.");
    if let Err(error) = axum::serve(listener, app).await {
        log::error!("Riftbound LAN server stopped: {error}");
    }
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../../../index.html"))
}

async fn health() -> (StatusCode, &'static str) {
    (StatusCode::OK, "Riftbound LAN host online")
}

async fn websocket(
    upgrade: WebSocketUpgrade,
    State(state): State<HostState>,
) -> impl axum::response::IntoResponse {
    upgrade
        .max_message_size(1024)
        .max_frame_size(1024)
        .on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: HostState) {
    let id = state.next_id.fetch_add(1, Ordering::Relaxed);
    let initial = PlayerState {
        id,
        x: WORLD_WIDTH * 0.5,
        y: WORLD_HEIGHT * 0.5,
        face: 0.0,
    };
    {
        let mut players = state.players.lock().await;
        if !can_accept_player(players.len()) {
            let (mut sender, _) = socket.split();
            let _ = sender.send(Message::Close(None)).await;
            return;
        }
        players.insert(id, initial);
    }

    let (mut sender, mut receiver) = socket.split();
    let mut last_input_at = Instant::now();
    let welcome = serde_json::json!({ "kind": "welcome", "self_id": id }).to_string();
    if sender.send(Message::Text(welcome.into())).await.is_err() {
        state.players.lock().await.remove(&id);
        return;
    }
    let mut updates = state.updates.subscribe();
    publish_snapshot(&state).await;
    let writer = tokio::spawn(async move {
        loop {
            let message = match updates.recv().await {
                Ok(message) => message,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            };
            if sender.send(Message::Text(message.into())).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(message)) = receiver.next().await {
        let text = match message {
            Message::Text(text) => text,
            Message::Close(_) => break,
            _ => continue,
        };
        let Ok(input) = serde_json::from_str::<ClientMessage>(text.as_str()) else {
            continue;
        };
        let _ = handle_client_message(&state, id, input, &mut last_input_at).await;
    }

    state.players.lock().await.remove(&id);
    {
        let mut driver = state.vehicle_driver.lock().await;
        if release_vehicle_driver(&mut driver, id) {
            state.vehicle.lock().await.speed = 0.0;
        }
    }
    publish_snapshot(&state).await;
    writer.abort();
}

#[allow(clippy::too_many_lines)]
async fn handle_client_message(
    state: &HostState,
    id: u64,
    input: ClientMessage,
    last_input_at: &mut Instant,
) -> bool {
    match input.kind.as_str() {
        "move" => {
            let (Some(dx), Some(dy), Some(face)) = (input.dx, input.dy, input.face) else {
                return false;
            };
            if !is_valid_move_input(dx, dy, face) {
                return false;
            }
            let now = Instant::now();
            let interval = now.duration_since(*last_input_at);
            if interval < MIN_MOVE_INTERVAL {
                return false;
            }
            let elapsed = interval.as_secs_f32().min(MAX_INPUT_DELTA);
            *last_input_at = now;
            let magnitude = dx.hypot(dy);
            let mut players = state.players.lock().await;
            if let Some(player) = players.get_mut(&id) {
                let scale = if magnitude > 1.0 {
                    1.0 / magnitude
                } else {
                    1.0
                };
                let (next_x, next_y) = move_player(
                    player.x,
                    player.y,
                    dx * scale * PLAYER_SPEED * elapsed,
                    dy * scale * PLAYER_SPEED * elapsed,
                );
                player.x = next_x;
                player.y = next_y;
                player.face = face;
                let player_x = player.x;
                let player_y = player.y;
                drop(players);
                let mut enemies = state.enemies.lock().await;
                for enemy in enemies.iter_mut().filter(|enemy| enemy.hp > 0) {
                    enemy.stun = (enemy.stun - elapsed).max(0.0);
                    if enemy.stun <= 0.0 {
                        let dx = player_x - enemy.x;
                        let dy = player_y - enemy.y;
                        let distance = dx.hypot(dy);
                        if distance < 430.0 {
                            let speed = if distance > 100.0 { 85.0 } else { 0.0 };
                            let step = speed * elapsed;
                            if distance > 0.001 {
                                enemy.x = (enemy.x + dx / distance * step)
                                    .clamp(50.0, WORLD_WIDTH - 50.0);
                                enemy.y = (enemy.y + dy / distance * step)
                                    .clamp(50.0, WORLD_HEIGHT - 50.0);
                            }
                        }
                    }
                }
            }
        }
        "enter_vehicle" => {
            let players = state.players.lock().await;
            let Some(player) = players.get(&id) else {
                return false;
            };
            let vehicle = state.vehicle.lock().await;
            if (player.x - vehicle.x).hypot(player.y - vehicle.y) > 90.0 {
                return false;
            }
            drop(vehicle);
            let mut driver = state.vehicle_driver.lock().await;
            if driver.is_some_and(|current| current != id) {
                return false;
            }
            *driver = Some(id);
        }
        "exit_vehicle" => {
            let mut driver = state.vehicle_driver.lock().await;
            if !release_vehicle_driver(&mut driver, id) {
                return false;
            }
            state.vehicle.lock().await.speed = 0.0;
        }
        "drive" => {
            let (Some(throttle), Some(brake), Some(steering)) =
                (input.throttle, input.brake, input.steering)
            else {
                return false;
            };
            if !is_valid_vehicle_input(throttle, brake, steering)
                || *state.vehicle_driver.lock().await != Some(id)
            {
                return false;
            }
            let now = Instant::now();
            let elapsed = now
                .duration_since(*last_input_at)
                .as_secs_f32()
                .min(MAX_INPUT_DELTA);
            if elapsed < MIN_MOVE_INTERVAL.as_secs_f32() {
                return false;
            }
            *last_input_at = now;
            let mut vehicle = state.vehicle.lock().await;
            update_shared_vehicle(&mut vehicle, throttle, brake, steering, elapsed);
        }
        "attack" => {
            let Some(index) = input.enemy else {
                return false;
            };
            let players = state.players.lock().await;
            let Some(player) = players.get(&id) else {
                return false;
            };
            let mut enemies = state.enemies.lock().await;
            let Some(enemy) = enemies
                .iter_mut()
                .find(|enemy| enemy.id == index && enemy.hp > 0)
            else {
                return false;
            };
            if (player.x - enemy.x).hypot(player.y - enemy.y) > 75.0 {
                return false;
            }
            enemy.hp = enemy.hp.saturating_sub(1);
            enemy.stun = 0.9;
        }
        "collect" => {
            let Some(index) = input.shard else {
                return false;
            };
            let players = state.players.lock().await;
            let Some(player) = players.get(&id) else {
                return false;
            };
            if index >= SHARD_POSITIONS.len() || !within_pickup_range(player.x, player.y, index) {
                return false;
            }
            drop(players);
            state.shards.lock().await[index] = true;
        }
        "extract" => {
            let players = state.players.lock().await;
            let Some(player) = players.get(&id) else {
                return false;
            };
            let in_range = within_extraction_range(player.x, player.y);
            drop(players);
            let all_collected = state.shards.lock().await.iter().all(|taken| *taken);
            if all_collected && in_range {
                *state.extracted.lock().await = true;
            } else {
                return false;
            }
        }
        _ => return false,
    }
    true
}

async fn publish_snapshot(state: &HostState) {
    let players: Vec<PlayerState> = state.players.lock().await.values().cloned().collect();
    let shards = *state.shards.lock().await;
    let extracted = *state.extracted.lock().await;
    let enemies = state.enemies.lock().await.clone();
    let vehicle = state.vehicle.lock().await.clone();
    let vehicle_driver = *state.vehicle_driver.lock().await;
    if let Ok(message) = serde_json::to_string(&Snapshot {
        kind: "snapshot",
        players: &players,
        shards: &shards,
        extracted,
        enemies: &enemies,
        vehicle: &vehicle,
        vehicle_driver,
    }) {
        let _ = state.updates.send(message);
    }
}

fn move_player(x: f32, y: f32, dx: f32, dy: f32) -> (f32, f32) {
    let mut next_x = clamp_world_position(x + dx, WORLD_WIDTH - 50.0).max(50.0);
    if WORLD_OBSTACLES
        .iter()
        .any(|wall| circle_intersects_rect(next_x, y, PLAYER_RADIUS, *wall))
    {
        next_x = x;
    }
    let mut next_y = clamp_world_position(y + dy, WORLD_HEIGHT - 50.0).max(50.0);
    if WORLD_OBSTACLES
        .iter()
        .any(|wall| circle_intersects_rect(next_x, next_y, PLAYER_RADIUS, *wall))
    {
        next_y = y;
    }
    (next_x, next_y)
}

fn circle_intersects_rect(x: f32, y: f32, radius: f32, rect: (f32, f32, f32, f32)) -> bool {
    let (left, top, width, height) = rect;
    let closest_x = x.clamp(left, left + width);
    let closest_y = y.clamp(top, top + height);
    let dx = x - closest_x;
    let dy = y - closest_y;
    dx * dx + dy * dy < radius * radius
}

fn discover_lan_ip() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:80").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}

fn within_pickup_range(x: f32, y: f32, index: usize) -> bool {
    let Some((shard_x, shard_y)) = SHARD_POSITIONS.get(index) else {
        return false;
    };
    (x - shard_x).hypot(y - shard_y) <= PICKUP_RANGE
}

fn within_extraction_range(x: f32, y: f32) -> bool {
    (x - EXTRACTION_POSITION.0).hypot(y - EXTRACTION_POSITION.1) <= 90.0
}

fn release_vehicle_driver(driver: &mut Option<u64>, player_id: u64) -> bool {
    if *driver == Some(player_id) {
        *driver = None;
        true
    } else {
        false
    }
}

fn is_valid_vehicle_input(throttle: f32, brake: f32, steering: f32) -> bool {
    throttle.is_finite()
        && brake.is_finite()
        && steering.is_finite()
        && (-1.0..=1.0).contains(&throttle)
        && (0.0..=1.0).contains(&brake)
        && (-1.0..=1.0).contains(&steering)
}

fn update_shared_vehicle(
    vehicle: &mut SharedVehicleState,
    throttle: f32,
    brake: f32,
    steering: f32,
    delta_seconds: f32,
) {
    if !is_valid_vehicle_input(throttle, brake, steering)
        || !delta_seconds.is_finite()
        || delta_seconds <= 0.0
        || !vehicle.x.is_finite()
        || !vehicle.y.is_finite()
        || !vehicle.heading.is_finite()
        || !vehicle.speed.is_finite()
    {
        return;
    }
    let dt = delta_seconds.min(MAX_INPUT_DELTA);
    vehicle.speed += throttle * 170.0 * dt;
    vehicle.speed *= 1.0 - (0.65 * dt).min(0.95);
    vehicle.speed *= 1.0 - brake * (5.0 * dt).min(1.0);
    vehicle.speed = vehicle.speed.clamp(-90.0, 260.0);
    let steering_factor = (vehicle.speed.abs() / 35.0).clamp(0.0, 1.0);
    vehicle.heading += steering * steering_factor * 1.9 * dt * vehicle.speed.signum();
    let dx = vehicle.heading.sin() * vehicle.speed * dt;
    let dy = -vehicle.heading.cos() * vehicle.speed * dt;
    let steps = (dx.abs().max(dy.abs()) / MAX_VEHICLE_COLLISION_STEP)
        .ceil()
        .max(1.0) as usize;
    let step_x = dx / steps as f32;
    let step_y = dy / steps as f32;
    for _ in 0..steps {
        let next_x = (vehicle.x + step_x).clamp(50.0, WORLD_WIDTH - 50.0);
        if !circle_hits_world_obstacle(next_x, vehicle.y, VEHICLE_RADIUS) {
            vehicle.x = next_x;
        } else {
            vehicle.speed = 0.0;
        }
        let next_y = (vehicle.y + step_y).clamp(50.0, WORLD_HEIGHT - 50.0);
        if !circle_hits_world_obstacle(vehicle.x, next_y, VEHICLE_RADIUS) {
            vehicle.y = next_y;
        } else {
            vehicle.speed = 0.0;
        }
    }
}

fn circle_hits_world_obstacle(x: f32, y: f32, radius: f32) -> bool {
    x <= 50.0 + radius
        || x >= WORLD_WIDTH - 50.0 - radius
        || y <= 50.0 + radius
        || y >= WORLD_HEIGHT - 50.0 - radius
        || WORLD_OBSTACLES
            .iter()
            .any(|wall| circle_intersects_rect(x, y, radius, *wall))
}

fn is_valid_move_input(dx: f32, dy: f32, face: f32) -> bool {
    dx.is_finite() && dy.is_finite() && face.is_finite() && dx.hypot(dy) <= 1.01
}

fn can_accept_player(current_players: usize) -> bool {
    current_players < MAX_PLAYERS
}

fn clamp_world_position(value: f32, maximum: f32) -> f32 {
    value.clamp(0.0, maximum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disconnect_releases_only_the_matching_vehicle_driver() {
        let mut driver = Some(42);
        assert!(!release_vehicle_driver(&mut driver, 7));
        assert_eq!(driver, Some(42));
        assert!(release_vehicle_driver(&mut driver, 42));
        assert_eq!(driver, None);
        assert!(!release_vehicle_driver(&mut driver, 42));
    }

    #[test]
    fn vehicle_input_is_bounded_and_rejects_non_finite_values() {
        assert!(is_valid_vehicle_input(1.0, 0.0, -1.0));
        assert!(is_valid_vehicle_input(-1.0, 1.0, 1.0));
        assert!(!is_valid_vehicle_input(1.1, 0.0, 0.0));
        assert!(!is_valid_vehicle_input(0.0, -0.1, 0.0));
        assert!(!is_valid_vehicle_input(0.0, 0.0, f32::NAN));
    }

    #[test]
    fn shared_vehicle_update_keeps_speed_and_position_bounded() {
        let mut vehicle = SharedVehicleState {
            x: WORLD_WIDTH * 0.5,
            y: WORLD_HEIGHT * 0.5,
            heading: 0.0,
            speed: 0.0,
        };
        for _ in 0..200 {
            update_shared_vehicle(&mut vehicle, 1.0, 0.0, 1.0, 0.05);
        }
        assert!(vehicle.speed <= 260.0);
        assert!(vehicle.speed >= -90.0);
        assert!((50.0..=WORLD_WIDTH - 50.0).contains(&vehicle.x));
        assert!((50.0..=WORLD_HEIGHT - 50.0).contains(&vehicle.y));
        assert!(vehicle.heading.is_finite());
    }

    #[test]
    fn vehicle_collision_stops_at_walls_and_world_edges() {
        assert!(circle_hits_world_obstacle(630.0, 400.0, VEHICLE_RADIUS));
        assert!(!circle_hits_world_obstacle(600.0, 400.0, VEHICLE_RADIUS));
        assert!(circle_hits_world_obstacle(60.0, 750.0, VEHICLE_RADIUS));
        assert!(circle_hits_world_obstacle(
            WORLD_WIDTH - 60.0,
            750.0,
            VEHICLE_RADIUS
        ));
    }

    #[test]
    fn vehicle_collision_substeps_prevent_tunneling_through_thin_walls() {
        let mut vehicle = SharedVehicleState {
            x: 600.0,
            y: 400.0,
            heading: std::f32::consts::FRAC_PI_2,
            speed: 260.0,
        };
        update_shared_vehicle(&mut vehicle, 0.0, 0.0, 0.0, 0.1);
        assert!(vehicle.x < 650.0 - VEHICLE_RADIUS);
        assert_eq!(vehicle.speed, 0.0);
    }

    #[test]
    fn movement_input_rejects_non_finite_and_excessive_values() {
        assert!(is_valid_move_input(0.0, 0.0, 0.0));
        assert!(is_valid_move_input(0.7, 0.7, 0.0));
        assert!(!is_valid_move_input(f32::NAN, 0.0, 0.0));
        assert!(!is_valid_move_input(0.0, f32::INFINITY, 0.0));
        assert!(!is_valid_move_input(0.0, 0.0, f32::NEG_INFINITY));
        assert!(!is_valid_move_input(1.1, 0.0, 0.0));
    }

    #[test]
    fn host_rejects_connections_after_player_capacity_is_reached() {
        assert!(can_accept_player(0));
        assert!(can_accept_player(MAX_PLAYERS - 1));
        assert!(!can_accept_player(MAX_PLAYERS));
        assert!(!can_accept_player(MAX_PLAYERS + 1));
    }

    #[test]
    fn world_position_is_bounded() {
        assert_eq!(
            clamp_world_position(-20.0, WORLD_WIDTH).to_bits(),
            0.0_f32.to_bits()
        );
        assert_eq!(
            clamp_world_position(WORLD_WIDTH + 20.0, WORLD_WIDTH).to_bits(),
            WORLD_WIDTH.to_bits()
        );
        assert_eq!(
            clamp_world_position(42.0, WORLD_WIDTH).to_bits(),
            42.0_f32.to_bits()
        );
    }

    #[test]
    fn movement_is_normalized_and_bounded_by_host_speed() {
        let magnitude = 1.0_f32.hypot(1.0);
        let scale = 1.0 / magnitude;
        let step = scale * PLAYER_SPEED * MAX_INPUT_DELTA;
        assert!((step.hypot(step) - PLAYER_SPEED * MAX_INPUT_DELTA).abs() < 0.001);
        assert_eq!(
            clamp_world_position(WORLD_WIDTH + step, WORLD_WIDTH),
            WORLD_WIDTH
        );
    }

    #[test]
    fn host_collision_blocks_walls_and_allows_wall_sliding() {
        let (blocked_x, blocked_y) = move_player(630.0, 400.0, 40.0, 0.0);
        assert_eq!(blocked_x, 630.0);
        assert_eq!(blocked_y, 400.0);
        let (slide_x, slide_y) = move_player(630.0, 400.0, 40.0, 50.0);
        assert_eq!(slide_x, 630.0);
        assert_eq!(slide_y, 450.0);
    }

    #[test]
    fn pickup_and_extraction_ranges_are_checked_on_host() {
        assert!(within_pickup_range(380.0, 330.0, 0));
        assert!(!within_pickup_range(900.0, 900.0, 0));
        assert!(!within_pickup_range(380.0, 330.0, 99));
        assert!(within_extraction_range(1100.0, 750.0));
        assert!(!within_extraction_range(300.0, 300.0));
    }
}
