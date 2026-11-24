//! Local-network HTTP and WebSocket host for browser-based Riftbound sessions.

use std::{collections::BTreeMap, net::SocketAddr, sync::{Arc, atomic::{AtomicU64, Ordering}}};

use axum::{
    Router,
    extract::{State, ws::{Message, WebSocket, WebSocketUpgrade}},
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

#[derive(Clone)]
struct HostState {
    players: Arc<Mutex<BTreeMap<u64, PlayerState>>>,
    updates: broadcast::Sender<String>,
    next_id: Arc<AtomicU64>,
}

#[derive(Clone, Serialize)]
struct PlayerState {
    id: u64,
    x: f32,
    y: f32,
    face: f32,
}

#[derive(Deserialize)]
struct ClientMessage {
    kind: String,
    x: f32,
    y: f32,
    face: f32,
}

#[derive(Serialize)]
struct Snapshot<'a> {
    kind: &'static str,
    self_id: u64,
    players: &'a [PlayerState],
}

/// Starts the LAN web host and multiplayer relay on all local network interfaces.
pub async fn serve() {
    let (updates, _) = broadcast::channel(128);
    let state = HostState {
        players: Arc::new(Mutex::new(BTreeMap::new())),
        updates,
        next_id: Arc::new(AtomicU64::new(1)),
    };
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
    log::info!("Riftbound LAN session ready: http://{lan_ip}:{PORT}");
    log::info!("Open the address on devices connected to the same Wi-Fi network.");
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

async fn websocket(upgrade: WebSocketUpgrade, State(state): State<HostState>) -> impl axum::response::IntoResponse {
    upgrade
        .max_message_size(1024)
        .max_frame_size(1024)
        .on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: HostState) {
    let id = state.next_id.fetch_add(1, Ordering::Relaxed);
    let initial = PlayerState { id, x: WORLD_WIDTH * 0.5, y: WORLD_HEIGHT * 0.5, face: 0.0 };
    state.players.lock().await.insert(id, initial);
    publish_snapshot(&state, id).await;

    let (mut sender, mut receiver) = socket.split();
    let mut updates = state.updates.subscribe();
    let writer = tokio::spawn(async move {
        while let Ok(message) = updates.recv().await {
            if sender.send(Message::Text(message.into())).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(message)) = receiver.next().await {
        let Message::Text(text) = message else {
            if matches!(message, Message::Close(_)) { break; }
            continue;
        };
        let Ok(input) = serde_json::from_str::<ClientMessage>(text.as_str()) else { continue; };
        if input.kind != "move" || !input.x.is_finite() || !input.y.is_finite() || !input.face.is_finite() {
            continue;
        }
        {
            let mut players = state.players.lock().await;
            if let Some(player) = players.get_mut(&id) {
                player.x = input.x.clamp(0.0, WORLD_WIDTH);
                player.y = input.y.clamp(0.0, WORLD_HEIGHT);
                player.face = input.face;
            }
        }
        publish_snapshot(&state, id).await;
    }

    state.players.lock().await.remove(&id);
    publish_snapshot(&state, 0).await;
    writer.abort();
}

async fn publish_snapshot(state: &HostState, self_id: u64) {
    let players: Vec<PlayerState> = state.players.lock().await.values().cloned().collect();
    if let Ok(message) = serde_json::to_string(&Snapshot { kind: "snapshot", self_id, players: &players }) {
        let _ = state.updates.send(message);
    }
}

fn discover_lan_ip() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:80").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}
