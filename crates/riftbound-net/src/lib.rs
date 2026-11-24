//! Local-network HTTP and WebSocket host for browser-based Riftbound sessions.

use std::{
    collections::BTreeMap,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
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
    players: &'a [PlayerState],
}

/// Starts the LAN web host and multiplayer relay on all local network interfaces.
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
    state.players.lock().await.insert(id, initial);

    let (mut sender, mut receiver) = socket.split();
    let welcome = serde_json::json!({ "kind": "welcome", "self_id": id }).to_string();
    if sender.send(Message::Text(welcome.into())).await.is_err() {
        state.players.lock().await.remove(&id);
        return;
    }
    let mut updates = state.updates.subscribe();
    publish_snapshot(&state).await;
    let writer = tokio::spawn(async move {
        while let Ok(message) = updates.recv().await {
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
        if input.kind != "move"
            || !input.x.is_finite()
            || !input.y.is_finite()
            || !input.face.is_finite()
        {
            continue;
        }
        {
            let mut players = state.players.lock().await;
            if let Some(player) = players.get_mut(&id) {
                player.x = clamp_world_position(input.x, WORLD_WIDTH);
                player.y = clamp_world_position(input.y, WORLD_HEIGHT);
                player.face = input.face;
            }
        }
        publish_snapshot(&state).await;
    }

    state.players.lock().await.remove(&id);
    publish_snapshot(&state).await;
    writer.abort();
}

async fn publish_snapshot(state: &HostState) {
    let players: Vec<PlayerState> = state.players.lock().await.values().cloned().collect();
    if let Ok(message) = serde_json::to_string(&Snapshot {
        kind: "snapshot",
        players: &players,
    }) {
        let _ = state.updates.send(message);
    }
}

fn discover_lan_ip() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:80").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}

fn clamp_world_position(value: f32, maximum: f32) -> f32 {
    value.clamp(0.0, maximum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_position_is_bounded() {
        assert_eq!(clamp_world_position(-20.0, WORLD_WIDTH).to_bits(), 0.0_f32.to_bits());
        assert_eq!(
            clamp_world_position(WORLD_WIDTH + 20.0, WORLD_WIDTH).to_bits(),
            WORLD_WIDTH.to_bits()
        );
        assert_eq!(clamp_world_position(42.0, WORLD_WIDTH).to_bits(), 42.0_f32.to_bits());
    }
}
