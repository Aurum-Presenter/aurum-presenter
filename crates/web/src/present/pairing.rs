//! Pairing a stage device with the control device over the local network.
//!
//! The data channel is peer-to-peer and DTLS-encrypted; the only thing that touches the server
//! is the offer, the answer and the ICE candidates, through a relay that holds nothing. Session
//! state — which includes copyrighted lyrics — never leaves the LAN.
//!
//! The two LAN-local signalling paths the stage-view document prefers (mDNS, and a local HTTP
//! endpoint on the control device) are not reachable from a browser: a page cannot advertise a
//! service or listen on a port. So the relay, which that document makes the last resort, is the
//! path a browser can actually take, and the change request that added it says exactly why.

use std::cell::RefCell;
use std::rc::Rc;

use aurum_core::present::session::SessionMessage;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    MessageEvent, RtcConfiguration, RtcDataChannel, RtcDataChannelEvent, RtcDataChannelInit,
    RtcIceCandidate, RtcIceCandidateInit, RtcPeerConnection, RtcPeerConnectionIceEvent, RtcSdpType,
    RtcSessionDescriptionInit, WebSocket,
};

use crate::api::Api;

/// The relay is its own process on its own port — it holds sockets open, which the API's request
/// lifecycle cannot — so it has its own base URL rather than being derived from the API's.
fn signal_url() -> String {
    option_env!("AURUM_SIGNAL_URL")
        .map(str::to_owned)
        .unwrap_or_else(|| "ws://localhost:8081".to_owned())
}

fn socket_url(code: &str, workspace_id: Option<&str>) -> String {
    let token = Api::access_token().unwrap_or_default();
    let workspace = workspace_id
        .map(|id| format!("&workspace={id}"))
        .unwrap_or_default();

    format!(
        "{}/api/v1/sessions/{code}/signal?token={token}{workspace}",
        signal_url(),
    )
}

/// What crosses the relay. SDP and ICE only; it never sees session content.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Signal {
    Offer { sdp: String },
    Answer { sdp: String },
    Ice { candidate: serde_json::Value },
}

/// No STUN, no TURN: both peers are on the same LAN by design, and host candidates are enough.
/// Adding a public STUN server would leak the fact of a session to a third party for nothing.
fn configuration() -> RtcConfiguration {
    let config = RtcConfiguration::new();

    config.set_ice_servers(&js_sys::Array::new());
    config
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Waiting,
    Connected,
    Failed,
}

/// One paired device, as the control surface sees it.
pub struct Peer {
    pub output_id: String,
    pub send: super::transport::Send,
    channel: RtcDataChannel,
    connection: RtcPeerConnection,
}

impl Peer {
    pub fn close(&self) {
        self.channel.close();
        self.connection.close();
    }
}

fn send_over(channel: &RtcDataChannel) -> super::transport::Send {
    let channel = channel.clone();

    Rc::new(move |message: &SessionMessage| {
        if channel.ready_state() == web_sys::RtcDataChannelState::Open
            && let Ok(text) = serde_json::to_string(message)
        {
            let _ = channel.send_with_str(&text);
        }
    })
}

async fn describe(
    connection: &RtcPeerConnection,
    kind: RtcSdpType,
    sdp: &str,
) -> Result<(), JsValue> {
    let description = RtcSessionDescriptionInit::new(kind);

    description.set_sdp(sdp);
    JsFuture::from(connection.set_remote_description(&description)).await?;

    Ok(())
}

fn sdp_of(description: &JsValue) -> Option<String> {
    js_sys::Reflect::get(description, &JsValue::from_str("sdp"))
        .ok()
        .and_then(|value| value.as_string())
}

/// The control device's side: waits on the relay for a device that has been given the code,
/// answers its offer, and hands back a channel that behaves like any other output.
pub struct PairingHost {
    socket: RefCell<Option<WebSocket>>,
    peers: RefCell<Vec<RtcPeerConnection>>,
}

impl PairingHost {
    pub fn listen(
        code: &str,
        workspace_id: &str,
        on_peer: Callback<Rc<Peer>>,
        // `on_message` receives the id the *control* gave this peer, not whatever the device
        // claims to be. A device cannot borrow another output's identity, which matters because
        // that identity is what carries the right to advance the session.
        on_message: Callback<(String, SessionMessage)>,
        on_status: Callback<Status>,
    ) -> Rc<PairingHost> {
        let host = Rc::new(PairingHost {
            socket: RefCell::new(None),
            peers: RefCell::new(Vec::new()),
        });

        let Ok(socket) = WebSocket::new(&socket_url(code, Some(workspace_id))) else {
            on_status.run(Status::Failed);

            return host;
        };

        *host.socket.borrow_mut() = Some(socket.clone());

        {
            let opened = Closure::<dyn Fn()>::new(move || on_status.run(Status::Waiting));
            socket.set_onopen(Some(opened.as_ref().unchecked_ref()));
            opened.forget();
        }

        {
            let failed = Closure::<dyn Fn()>::new(move || on_status.run(Status::Failed));
            socket.set_onerror(Some(failed.as_ref().unchecked_ref()));
            failed.forget();
        }

        {
            let held = host.clone();
            let listening = socket.clone();

            let incoming = Closure::<dyn Fn(MessageEvent)>::new(move |event: MessageEvent| {
                let Some(text) = event.data().as_string() else {
                    return;
                };

                let Ok(Signal::Offer { sdp }) = serde_json::from_str::<Signal>(&text) else {
                    return;
                };

                held.answer(&listening, sdp, on_peer, on_message, on_status);
            });

            socket.set_onmessage(Some(incoming.as_ref().unchecked_ref()));
            incoming.forget();
        }

        host
    }

    fn answer(
        &self,
        socket: &WebSocket,
        offer: String,
        on_peer: Callback<Rc<Peer>>,
        on_message: Callback<(String, SessionMessage)>,
        on_status: Callback<Status>,
    ) {
        let Ok(connection) = RtcPeerConnection::new_with_configuration(&configuration()) else {
            on_status.run(Status::Failed);
            return;
        };

        self.peers.borrow_mut().push(connection.clone());

        {
            let socket = socket.clone();

            let on_ice = Closure::<dyn Fn(RtcPeerConnectionIceEvent)>::new(
                move |event: RtcPeerConnectionIceEvent| {
                    let Some(candidate) = event.candidate() else {
                        return;
                    };

                    if let Ok(value) = serde_wasm_bindgen::from_value(candidate.to_json().into())
                        && let Ok(text) = serde_json::to_string(&Signal::Ice { candidate: value })
                    {
                        let _ = socket.send_with_str(&text);
                    }
                },
            );

            connection.set_onicecandidate(Some(on_ice.as_ref().unchecked_ref()));
            on_ice.forget();
        }

        {
            let socket = socket.clone();
            let held = connection.clone();

            let on_channel =
                Closure::<dyn Fn(RtcDataChannelEvent)>::new(move |event: RtcDataChannelEvent| {
                    let channel = event.channel();
                    let output_id = crate::new_id();

                    {
                        let output_id = output_id.clone();

                        let incoming =
                            Closure::<dyn Fn(MessageEvent)>::new(move |event: MessageEvent| {
                                let Some(text) = event.data().as_string() else {
                                    return;
                                };

                                if let Ok(message) = serde_json::from_str::<SessionMessage>(&text) {
                                    on_message.run((output_id.clone(), message));
                                }
                            });

                        channel.set_onmessage(Some(incoming.as_ref().unchecked_ref()));
                        incoming.forget();
                    }

                    {
                        let (opening, held, relay) =
                            (channel.clone(), held.clone(), socket.clone());

                        let opened = Closure::<dyn Fn()>::new(move || {
                            on_status.run(Status::Connected);
                            on_peer.run(Rc::new(Peer {
                                output_id: output_id.clone(),
                                send: send_over(&opening),
                                channel: opening.clone(),
                                connection: held.clone(),
                            }));

                            // The relay is a setup channel with a short life: once the data
                            // channel is open there is nothing left for the server to carry.
                            let _ = relay.close();
                        });

                        channel.set_onopen(Some(opened.as_ref().unchecked_ref()));
                        opened.forget();
                    }
                });

            connection.set_ondatachannel(Some(on_channel.as_ref().unchecked_ref()));
            on_channel.forget();
        }

        let socket = socket.clone();

        leptos::task::spawn_local(async move {
            if describe(&connection, RtcSdpType::Offer, &offer)
                .await
                .is_err()
            {
                on_status.run(Status::Failed);
                return;
            }

            let Ok(answer) = JsFuture::from(connection.create_answer()).await else {
                on_status.run(Status::Failed);
                return;
            };

            let Some(sdp) = sdp_of(&answer) else {
                on_status.run(Status::Failed);
                return;
            };

            let local = RtcSessionDescriptionInit::new(RtcSdpType::Answer);
            local.set_sdp(&sdp);

            if JsFuture::from(connection.set_local_description(&local))
                .await
                .is_err()
            {
                on_status.run(Status::Failed);
                return;
            }

            if let Ok(text) = serde_json::to_string(&Signal::Answer { sdp }) {
                let _ = socket.send_with_str(&text);
            }
        });
    }

    pub fn close(&self) {
        if let Some(socket) = self.socket.borrow().as_ref() {
            let _ = socket.close();
        }

        for connection in self.peers.borrow().iter() {
            connection.close();
        }
    }
}

/// Why joining failed, in words a musician standing in a hall can act on.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum JoinError {
    #[error("The session could not be reached. Are you on the same network and signed in?")]
    Unreachable,
    #[error("That code is not a running session, or it has expired.")]
    NoSuchSession,
    #[error("Sign in again to join this session.")]
    SignedOut,
    #[error("Another device is already joining. Try again in a moment.")]
    Taken,
    #[error("The session did not answer. It may have ended.")]
    NoAnswer,
}

/// How long a joining device waits for the host to answer before giving up.
const JOIN_TIMEOUT_MS: u32 = 20_000;

/// The joining device's side: offers, waits for the answer, then talks peer-to-peer.
pub async fn join(
    code: &str,
    on_message: Callback<SessionMessage>,
    on_close: Callback<()>,
) -> Result<super::transport::Send, JoinError> {
    let connection = RtcPeerConnection::new_with_configuration(&configuration())
        .map_err(|_| JoinError::Unreachable)?;

    let options = RtcDataChannelInit::new();
    options.set_ordered(true);

    let channel = connection.create_data_channel_with_data_channel_dict("session", &options);

    {
        let incoming = Closure::<dyn Fn(MessageEvent)>::new(move |event: MessageEvent| {
            if let Some(text) = event.data().as_string()
                && let Ok(message) = serde_json::from_str::<SessionMessage>(&text)
            {
                on_message.run(message);
            }
        });

        channel.set_onmessage(Some(incoming.as_ref().unchecked_ref()));
        incoming.forget();
    }

    {
        let closed = Closure::<dyn Fn()>::new(move || on_close.run(()));
        channel.set_onclose(Some(closed.as_ref().unchecked_ref()));
        closed.forget();
    }

    let socket = WebSocket::new(&socket_url(code, None)).map_err(|_| JoinError::Unreachable)?;
    let failure: Rc<RefCell<Option<JoinError>>> = Rc::default();

    // The relay closes with a code that says exactly what went wrong, and those four sentences
    // are the difference between "it didn't work" and knowing what to do about it.
    {
        let failure = failure.clone();

        let closed =
            Closure::<dyn Fn(web_sys::CloseEvent)>::new(move |event: web_sys::CloseEvent| {
                *failure.borrow_mut() = Some(match event.code() {
                    4404 => JoinError::NoSuchSession,
                    4401 => JoinError::SignedOut,
                    4409 => JoinError::Taken,
                    _ => JoinError::Unreachable,
                });
            });

        socket.set_onclose(Some(closed.as_ref().unchecked_ref()));
        closed.forget();
    }

    open(&socket)
        .await
        .map_err(|_| failure.borrow().clone().unwrap_or(JoinError::Unreachable))?;

    {
        let socket = socket.clone();

        let on_ice = Closure::<dyn Fn(RtcPeerConnectionIceEvent)>::new(
            move |event: RtcPeerConnectionIceEvent| {
                let Some(candidate) = event.candidate() else {
                    return;
                };

                if socket.ready_state() == WebSocket::OPEN
                    && let Ok(value) = serde_wasm_bindgen::from_value(candidate.to_json().into())
                    && let Ok(text) = serde_json::to_string(&Signal::Ice { candidate: value })
                {
                    let _ = socket.send_with_str(&text);
                }
            },
        );

        connection.set_onicecandidate(Some(on_ice.as_ref().unchecked_ref()));
        on_ice.forget();
    }

    {
        let connection = connection.clone();

        let incoming = Closure::<dyn Fn(MessageEvent)>::new(move |event: MessageEvent| {
            let Some(text) = event.data().as_string() else {
                return;
            };

            let connection = connection.clone();

            leptos::task::spawn_local(async move {
                match serde_json::from_str::<Signal>(&text) {
                    Ok(Signal::Answer { sdp }) => {
                        let _ = describe(&connection, RtcSdpType::Answer, &sdp).await;
                    }

                    Ok(Signal::Ice { candidate }) => {
                        if let Ok(value) = serde_wasm_bindgen::to_value(&candidate) {
                            let init = RtcIceCandidateInit::from(value);

                            if let Ok(candidate) = RtcIceCandidate::new(&init) {
                                let _ = JsFuture::from(
                                    connection.add_ice_candidate_with_opt_rtc_ice_candidate(Some(
                                        &candidate,
                                    )),
                                )
                                .await;
                            }
                        }
                    }

                    _ => {}
                }
            });
        });

        socket.set_onmessage(Some(incoming.as_ref().unchecked_ref()));
        incoming.forget();
    }

    let offer = JsFuture::from(connection.create_offer())
        .await
        .map_err(|_| JoinError::Unreachable)?;
    let sdp = sdp_of(&offer).ok_or(JoinError::Unreachable)?;

    let local = RtcSessionDescriptionInit::new(RtcSdpType::Offer);
    local.set_sdp(&sdp);

    JsFuture::from(connection.set_local_description(&local))
        .await
        .map_err(|_| JoinError::Unreachable)?;

    if let Ok(text) = serde_json::to_string(&Signal::Offer { sdp }) {
        let _ = socket.send_with_str(&text);
    }

    channel_open(&channel).await.ok_or(JoinError::NoAnswer)?;

    let _ = socket.close();

    Ok(send_over(&channel))
}

/// Waits for a socket to open, or for it to fail.
async fn open(socket: &WebSocket) -> Result<(), ()> {
    let ready: Rc<RefCell<Option<bool>>> = Rc::default();

    {
        let ready = ready.clone();
        let opened = Closure::<dyn Fn()>::new(move || *ready.borrow_mut() = Some(true));
        socket.set_onopen(Some(opened.as_ref().unchecked_ref()));
        opened.forget();
    }

    {
        let ready = ready.clone();
        let failed = Closure::<dyn Fn()>::new(move || *ready.borrow_mut() = Some(false));
        socket.set_onerror(Some(failed.as_ref().unchecked_ref()));
        failed.forget();
    }

    for _ in 0..(JOIN_TIMEOUT_MS / 50) {
        match *ready.borrow() {
            Some(true) => return Ok(()),
            Some(false) => return Err(()),
            None => {}
        }

        if socket.ready_state() == WebSocket::CLOSED {
            return Err(());
        }

        gloo_timers::future::TimeoutFuture::new(50).await;
    }

    Err(())
}

async fn channel_open(channel: &RtcDataChannel) -> Option<()> {
    for _ in 0..(JOIN_TIMEOUT_MS / 50) {
        if channel.ready_state() == web_sys::RtcDataChannelState::Open {
            return Some(());
        }

        gloo_timers::future::TimeoutFuture::new(50).await;
    }

    None
}
