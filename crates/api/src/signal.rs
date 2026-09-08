//! The signalling relay: a post box for two devices trying to find each other on a LAN.
//!
//! It holds no session state, stores no message, and never sees a slide, a lyric or a chord. A
//! socket lives until the data channel opens or two minutes pass, whichever comes first.
//!
//! The PHP wrote 669 lines of RFC 6455 by hand — framing, masking, `stream_select` — because
//! PHP-FPM cannot hold a socket open and the API must not be blocked by one. Both halves of that
//! reasoning go away here: `tokio-tungstenite` does the framing, and the relay is a task in the
//! same process as the API rather than a second binary.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};

use crate::db::{blocking, now_ms};
use crate::repo::workspaces;
use crate::state::AppState;

/// A pairing that has not completed in this long is abandoned.
const ROOM_TTL_SECONDS: i64 = 120;
/// Generous for a human plugging in a tablet, mean for anything automated.
const MAX_ATTEMPTS_PER_MINUTE: usize = 20;
const MAX_PAYLOAD: usize = 16 * 1024;

type Peer = mpsc::UnboundedSender<Message>;

#[derive(Default)]
struct Rooms {
    /// Code → the workspace its host declared, the host, the guest, and when it opened.
    open: HashMap<String, Room>,
    /// User → recent handshake times, for the rate limit.
    attempts: HashMap<String, Vec<i64>>,
}

struct Room {
    workspace_id: String,
    host: Peer,
    guest: Option<Peer>,
    opened_at: i64,
}

/// Who is paired with whom, for as long as it takes to pair them. The whole of the relay's
/// state: in memory, and gone when the process restarts — which is exactly right for a post box.
#[derive(Clone, Default)]
pub struct Relay(Arc<Mutex<Rooms>>);

impl Relay {
    fn allow_attempt(&self, user_id: &str, now: i64) -> bool {
        let mut rooms = self.0.lock().expect("relay state");
        let recent = rooms.attempts.entry(user_id.to_owned()).or_default();

        recent.retain(|at| now - *at < 60);
        recent.push(now);

        recent.len() <= MAX_ATTEMPTS_PER_MINUTE
    }

    fn workspace_of(&self, code: &str) -> Option<String> {
        let rooms = self.0.lock().expect("relay state");

        rooms.open.get(code).map(|room| room.workspace_id.clone())
    }

    fn open(&self, code: &str, workspace_id: &str, host: Peer, now: i64) {
        self.0.lock().expect("relay state").open.insert(
            code.to_owned(),
            Room {
                workspace_id: workspace_id.to_owned(),
                host,
                guest: None,
                opened_at: now,
            },
        );
    }

    /// At most two peers per code. A third is refused rather than queued, so a stray tab cannot
    /// take the place of the tablet somebody is holding.
    fn join(&self, code: &str, guest: Peer) -> bool {
        let mut rooms = self.0.lock().expect("relay state");

        match rooms.open.get_mut(code) {
            Some(room) if room.guest.is_none() => {
                room.guest = Some(guest);
                true
            }
            _ => false,
        }
    }

    /// The other end of the room, which is the only place a message may go.
    fn peer_of(&self, code: &str, is_host: bool) -> Option<Peer> {
        let rooms = self.0.lock().expect("relay state");
        let room = rooms.open.get(code)?;

        if is_host {
            room.guest.clone()
        } else {
            Some(room.host.clone())
        }
    }

    fn close(&self, code: &str) {
        self.0.lock().expect("relay state").open.remove(code);
    }

    fn expired(&self, now: i64) -> Vec<String> {
        let rooms = self.0.lock().expect("relay state");

        rooms
            .open
            .iter()
            .filter(|(_, room)| now - room.opened_at >= ROOM_TTL_SECONDS)
            .map(|(code, _)| code.clone())
            .collect()
    }
}

/// Only an offer, an answer or a candidate. Anything else and the socket is closed rather than
/// asked again: whatever is on the other end is not the app.
fn is_signal(payload: &str) -> bool {
    if payload.len() > MAX_PAYLOAD {
        return false;
    }

    let Ok(decoded) = serde_json::from_str::<Value>(payload) else {
        return false;
    };

    match decoded.get("kind").and_then(Value::as_str) {
        Some("offer" | "answer") => decoded.get("sdp").is_some_and(Value::is_string),
        Some("ice") => decoded
            .get("candidate")
            .is_some_and(|candidate| candidate.is_object() || candidate.is_array()),
        _ => false,
    }
}

/// The pairing code out of `/api/v1/sessions/{code}/signal`.
///
/// Codes are typed by a person on a dark stage, so they arrive in whatever case the keyboard was
/// in; the alphabet itself has no ambiguous characters.
fn code_of(path: &str) -> Option<String> {
    let code = path
        .strip_prefix("/api/v1/sessions/")?
        .strip_suffix("/signal")?;

    (code.len() == 6
        && code
            .bytes()
            .all(|byte| byte.is_ascii_alphabetic() || ('2'..='9').contains(&(byte as char))))
    .then(|| code.to_uppercase())
}

fn query_of(uri: &str) -> HashMap<String, String> {
    uri.split_once('?')
        .map(|(_, query)| query)
        .unwrap_or_default()
        .split('&')
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=')?;

            Some((
                urlencoding::decode(key).ok()?.into_owned(),
                urlencoding::decode(value).ok()?.into_owned(),
            ))
        })
        .collect()
}

pub async fn serve(state: AppState, bind: &str) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(bind).await?;
    let relay = Relay::default();

    tracing::info!("signalling relay listening on {bind}");

    // A pairing nobody completed is swept rather than left holding a code somebody else wants.
    {
        let relay = relay.clone();

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;

                for code in relay.expired(now_ms() / 1000) {
                    relay.close(&code);
                }
            }
        });
    }

    loop {
        let Ok((stream, _)) = listener.accept().await else {
            continue;
        };
        let (state, relay) = (state.clone(), relay.clone());

        tokio::spawn(async move {
            if let Err(error) = connection(state, relay, stream).await {
                tracing::debug!("signalling connection closed: {error}");
            }
        });
    }
}

async fn connection(
    state: AppState,
    relay: Relay,
    stream: TcpStream,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut target = String::new();

    // The upgrade itself always succeeds; every refusal below is a close frame instead, so the
    // client can read a reason rather than a bare failed connection. The callback exists only to
    // capture the target, whose query string carries the token and the workspace.
    //
    // The allow is for the library's error type, which is a whole HTTP response.
    #[allow(clippy::result_large_err)]
    let handshake = |request: &Request, response: Response| -> Result<Response, ErrorResponse> {
        target = request.uri().to_string();

        Ok(response)
    };

    let socket = tokio_tungstenite::accept_hdr_async(stream, handshake).await?;

    let path = target.split('?').next().unwrap_or_default().to_owned();
    let query = query_of(&target);

    let Some(code) = code_of(&path) else {
        return close(socket, 4404, "No such session.").await;
    };

    // The token arrives in the query string rather than a header because a browser will not let
    // a page set headers on a WebSocket handshake. It is the fifteen-minute access token, on a
    // socket that lives two minutes, over the same TLS as the rest of the API.
    let Some(claims) = state.tokens.verify_access_token(
        query.get("token").map(String::as_str).unwrap_or_default(),
        now_ms(),
    ) else {
        return close(socket, 4401, "Unauthorized.").await;
    };

    if !relay.allow_attempt(&claims.sub, now_ms() / 1000) {
        return close(socket, 4429, "Too many attempts.").await;
    }

    // The host opens the room and declares which workspace the session belongs to; the guest is
    // then checked against that same membership. Nothing about the session is stored.
    let is_host = relay.workspace_of(&code).is_none();
    let workspace_id = match relay.workspace_of(&code) {
        Some(id) => id,
        None => query.get("workspace").cloned().unwrap_or_default(),
    };

    if workspace_id.is_empty() {
        return close(socket, 4403, "Forbidden.").await;
    }

    let allowed = {
        let (state, user_id, workspace_id) =
            (state.clone(), claims.sub.clone(), workspace_id.clone());

        blocking(move || {
            Ok(workspaces::role_of(&state.db.open_control()?, &user_id, &workspace_id)?.is_some())
        })
        .await
        .unwrap_or(false)
    };

    if !allowed {
        return close(socket, 4403, "Forbidden.").await;
    }

    let (mut writer, mut reader) = socket.split();
    let (sender, mut inbox) = mpsc::unbounded_channel();

    if is_host {
        relay.open(&code, &workspace_id, sender, now_ms() / 1000);
    } else if !relay.join(&code, sender) {
        let mut socket = writer.reunite(reader)?;

        return close_split(
            &mut socket,
            4409,
            "That session already has a device joining.",
        )
        .await;
    }

    let pump = tokio::spawn(async move {
        while let Some(message) = inbox.recv().await {
            if writer.send(message).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(message)) = reader.next().await {
        match message {
            Message::Text(payload) if is_signal(&payload) => {
                if let Some(peer) = relay.peer_of(&code, is_host) {
                    let _ = peer.send(Message::Text(payload));
                }
            }
            Message::Text(_) | Message::Binary(_) => break,
            Message::Close(_) => break,
            _ => {}
        }
    }

    // Either end leaving ends the pairing: a half-open room is not something the other device
    // can do anything with.
    if let Some(peer) = relay.peer_of(&code, is_host) {
        let _ = peer.send(Message::Close(Some(
            tokio_tungstenite::tungstenite::protocol::CloseFrame {
                code: 4410u16.into(),
                reason: "The other device left.".into(),
            },
        )));
    }

    relay.close(&code);
    pump.abort();

    Ok(())
}

async fn close(
    mut socket: tokio_tungstenite::WebSocketStream<TcpStream>,
    code: u16,
    reason: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    close_split(&mut socket, code, reason).await
}

async fn close_split(
    socket: &mut tokio_tungstenite::WebSocketStream<TcpStream>,
    code: u16,
    reason: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let _ = socket
        .close(Some(tokio_tungstenite::tungstenite::protocol::CloseFrame {
            code: code.into(),
            reason: reason.to_owned().into(),
        }))
        .await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_pairing_code_out_of_the_path_in_any_case() {
        assert_eq!(
            code_of("/api/v1/sessions/4KJ9QP/signal").as_deref(),
            Some("4KJ9QP")
        );
        assert_eq!(
            code_of("/api/v1/sessions/4kj9qp/signal").as_deref(),
            Some("4KJ9QP")
        );
        assert_eq!(code_of("/api/v1/sessions/4KJ9Q/signal"), None, "too short");
        assert_eq!(
            code_of("/api/v1/sessions/4KJ9Q0/signal"),
            None,
            "zero is not in the alphabet"
        );
        assert_eq!(code_of("/anything/else"), None);
    }

    #[test]
    fn relays_only_the_three_messages_a_handshake_is_made_of() {
        assert!(is_signal(r#"{"kind":"offer","sdp":"v=0"}"#));
        assert!(is_signal(r#"{"kind":"answer","sdp":"v=0"}"#));
        assert!(is_signal(
            r#"{"kind":"ice","candidate":{"candidate":"a=x"}}"#
        ));

        assert!(!is_signal(r#"{"kind":"offer"}"#), "no sdp");
        assert!(!is_signal(r#"{"kind":"chat","text":"hello"}"#));
        assert!(!is_signal("not json"));
        assert!(!is_signal(&format!(
            r#"{{"kind":"offer","sdp":"{}"}}"#,
            "x".repeat(MAX_PAYLOAD)
        )));
    }

    #[test]
    fn reads_the_query_a_browser_sends() {
        let query = query_of("/api/v1/sessions/ABC123/signal?token=v1.a.b&workspace=ws-1");

        assert_eq!(query.get("token").map(String::as_str), Some("v1.a.b"));
        assert_eq!(query.get("workspace").map(String::as_str), Some("ws-1"));
        assert!(query_of("/no/query").is_empty());
    }

    /// Two peers per code, and the third is refused rather than queued.
    #[test]
    fn a_room_holds_exactly_two() {
        let relay = Relay::default();
        let (host, _host_inbox) = mpsc::unbounded_channel();
        let (guest, _guest_inbox) = mpsc::unbounded_channel();
        let (third, _third_inbox) = mpsc::unbounded_channel();

        relay.open("ABC123", "ws-1", host, 0);

        assert!(relay.join("ABC123", guest));
        assert!(!relay.join("ABC123", third));
        assert_eq!(relay.workspace_of("ABC123").as_deref(), Some("ws-1"));
    }

    #[test]
    fn a_pairing_nobody_completed_is_swept() {
        let relay = Relay::default();
        let (host, _inbox) = mpsc::unbounded_channel();

        relay.open("ABC123", "ws-1", host, 1000);

        assert!(relay.expired(1000 + ROOM_TTL_SECONDS - 1).is_empty());
        assert_eq!(relay.expired(1000 + ROOM_TTL_SECONDS), ["ABC123"]);
    }

    #[test]
    fn refuses_more_handshakes_than_a_person_could_be_making() {
        let relay = Relay::default();

        for attempt in 1..=MAX_ATTEMPTS_PER_MINUTE {
            assert!(relay.allow_attempt("ada", 1000), "attempt {attempt}");
        }

        assert!(!relay.allow_attempt("ada", 1000));
        // Somebody else is unaffected, and a minute later so is Ada.
        assert!(relay.allow_attempt("grace", 1000));
        assert!(relay.allow_attempt("ada", 1061));
    }
}
