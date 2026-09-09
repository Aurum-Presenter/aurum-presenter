//! Getting the state to every screen.
//!
//! Windows on the same device use a `BroadcastChannel`: no permission, no network, no server,
//! and nothing to fail on a venue's guest wifi. Devices on the same LAN use a WebRTC data
//! channel, set up through the signalling relay and then peer-to-peer.
//!
//! Both carry the same messages, so nothing above this file knows which one it is talking to.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use aurum_core::present::session::{
    OutputKind, OutputStatus, SessionMessage, SessionState, channel_name,
};
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::prelude::*;
use web_sys::{BroadcastChannel, MessageEvent};

/// An output that has not acked within this long is shown as not responding.
pub const NOT_RESPONDING_MS: i64 = 2000;
const HEARTBEAT_MS: u32 = 1000;

/// How a message leaves this device for one paired peer.
pub type Send = Rc<dyn Fn(&SessionMessage)>;

fn post(channel: &BroadcastChannel, message: &SessionMessage) {
    if let Ok(value) = serde_wasm_bindgen::to_value(message) {
        let _ = channel.post_message(&value);
    }
}

fn listen(channel: &BroadcastChannel, mut on_message: impl FnMut(SessionMessage) + 'static) {
    let handler = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
        // Anything that is not one of ours is not ours to worry about.
        if let Ok(message) = serde_wasm_bindgen::from_value::<SessionMessage>(event.data()) {
            on_message(message);
        }
    });

    channel.set_onmessage(Some(handler.as_ref().unchecked_ref()));
    handler.forget();
}

/// The control surface's side: one writer, many outputs.
#[derive(Clone)]
pub struct ControlTransport {
    channel: BroadcastChannel,
    inner: Rc<RefCell<Inner>>,
}

#[derive(Default)]
struct Inner {
    peers: HashMap<String, Send>,
    outputs: HashMap<String, OutputStatus>,
    seen: HashMap<String, i64>,
    state: Option<SessionState>,
    closed: bool,
}

impl ControlTransport {
    pub fn open(
        session_id: &str,
        on_outputs: Callback<Vec<OutputStatus>>,
        on_advance: Callback<(i64, String)>,
    ) -> Option<ControlTransport> {
        let channel = BroadcastChannel::new(&channel_name(session_id)).ok()?;
        let transport = ControlTransport {
            channel,
            inner: Rc::default(),
        };

        {
            let held = transport.clone();

            listen(&transport.channel, move |message| {
                held.receive(message, on_outputs, on_advance);
            });
        }

        // Anything that has not acked recently is marked, without touching the others.
        {
            let held = transport.clone();

            spawn_local(async move {
                loop {
                    gloo_timers::future::TimeoutFuture::new(HEARTBEAT_MS).await;

                    if held.inner.borrow().closed {
                        return;
                    }

                    if held.sweep() {
                        on_outputs.run(held.outputs());
                    }
                }
            });
        }

        Some(transport)
    }

    /// Adds a paired device's data channel as another output of the same session.
    pub fn attach_peer(&self, output_id: &str, send: Send) {
        let state = {
            let mut inner = self.inner.borrow_mut();

            inner.peers.insert(output_id.to_owned(), send.clone());
            inner.state.clone()
        };

        if let Some(state) = state {
            send(&SessionMessage::State {
                state: Box::new(state),
            });
        }
    }

    pub fn detach_peer(&self, output_id: &str) -> Vec<OutputStatus> {
        let mut inner = self.inner.borrow_mut();

        inner.peers.remove(output_id);
        inner.outputs.remove(output_id);

        inner.outputs.values().cloned().collect()
    }

    pub fn broadcast(&self, state: &SessionState) {
        let peers: Vec<Send> = {
            let mut inner = self.inner.borrow_mut();

            inner.state = Some(state.clone());
            inner.peers.values().cloned().collect()
        };

        let message = SessionMessage::State {
            state: Box::new(state.clone()),
        };

        post(&self.channel, &message);

        for send in peers {
            // A dead channel is not a reason to stop updating the others (acceptance
            // criterion 6): the projector keeps working while a paired tablet is asleep.
            send(&message);
        }
    }

    /// Only one device may hold the grant at a time (stage-view business rule 8).
    pub fn grant_advance(&self, output_id: &str, allowed: bool) -> Vec<OutputStatus> {
        let mut inner = self.inner.borrow_mut();

        for (id, output) in inner.outputs.iter_mut() {
            output.can_advance = allowed && id == output_id;
        }

        inner.outputs.values().cloned().collect()
    }

    pub fn outputs(&self) -> Vec<OutputStatus> {
        self.inner.borrow().outputs.values().cloned().collect()
    }

    pub fn close(&self) {
        {
            let mut inner = self.inner.borrow_mut();

            inner.closed = true;
            inner.peers.clear();
        }

        self.channel.close();
    }

    /// Handles one message from any wire. Public so a paired peer's data channel can feed the
    /// same code path as the broadcast channel.
    pub fn receive(
        &self,
        message: SessionMessage,
        on_outputs: Callback<Vec<OutputStatus>>,
        on_advance: Callback<(i64, String)>,
    ) {
        match message {
            SessionMessage::Hello {
                output_id,
                kind,
                label,
            } => {
                {
                    let mut inner = self.inner.borrow_mut();

                    inner.outputs.insert(
                        output_id.clone(),
                        OutputStatus {
                            output_id: output_id.clone(),
                            kind,
                            label,
                            joined_at: crate::now(),
                            // Nothing acked yet, and revision 0 is a real revision.
                            last_ack_revision: 0,
                            responding: true,
                            can_advance: false,
                        },
                    );

                    inner.seen.insert(output_id, crate::now_ms());
                }

                // Bound first, deliberately: a borrow taken in an `if let` scrutinee lives
                // until the end of the whole expression, so broadcasting from inside one would
                // ask for a second, mutable borrow of a cell it is still holding.
                let held = self.inner.borrow().state.clone();

                if let Some(state) = held {
                    self.broadcast(&state);
                }

                on_outputs.run(self.outputs());
            }

            SessionMessage::Ack {
                output_id,
                kind,
                label,
                revision,
            } => {
                {
                    let mut inner = self.inner.borrow_mut();
                    let existing = inner.outputs.get(&output_id);
                    let joined_at = existing
                        .map(|output| output.joined_at.clone())
                        .unwrap_or_else(crate::now);
                    let can_advance = existing.is_some_and(|output| output.can_advance);

                    inner.outputs.insert(
                        output_id.clone(),
                        OutputStatus {
                            output_id: output_id.clone(),
                            kind,
                            label,
                            joined_at,
                            last_ack_revision: revision,
                            responding: true,
                            can_advance,
                        },
                    );

                    inner.seen.insert(output_id, crate::now_ms());
                }

                on_outputs.run(self.outputs());
            }

            SessionMessage::Advance { output_id, delta } => {
                let allowed = self
                    .inner
                    .borrow()
                    .outputs
                    .get(&output_id)
                    .is_some_and(|output| output.can_advance);

                if allowed {
                    on_advance.run((delta, output_id));
                }
            }

            SessionMessage::RequestState { .. } => {
                let held = self.inner.borrow().state.clone();

                if let Some(state) = held {
                    self.broadcast(&state);
                }
            }

            SessionMessage::Bye { output_id } => {
                self.inner.borrow_mut().outputs.remove(&output_id);
                on_outputs.run(self.outputs());
            }

            // The control surface does not listen to itself.
            SessionMessage::State { .. } => {}
        }
    }

    fn sweep(&self) -> bool {
        let now = crate::now_ms();
        let mut inner = self.inner.borrow_mut();
        let mut changed = false;

        let seen = inner.seen.clone();

        for (id, output) in inner.outputs.iter_mut() {
            let responding = now - seen.get(id).copied().unwrap_or(0) < NOT_RESPONDING_MS;

            if responding != output.responding {
                output.responding = responding;
                changed = true;
            }
        }

        changed
    }
}

/// An output's side: receive state, ack it, and say hello loudly enough to be counted.
#[derive(Clone)]
pub struct OutputTransport {
    channel: BroadcastChannel,
    output_id: String,
    kind: OutputKind,
    label: String,
    /// The highest revision this output has applied. An older one is a message that overtook a
    /// newer one, and applying it would move the screen backwards.
    revision: Rc<RefCell<Option<u64>>>,
    closed: Rc<RefCell<bool>>,
}

impl OutputTransport {
    pub fn open(
        session_id: &str,
        kind: OutputKind,
        label: &str,
        on_state: Callback<SessionState>,
    ) -> Option<OutputTransport> {
        let channel = BroadcastChannel::new(&channel_name(session_id)).ok()?;

        let transport = OutputTransport {
            channel,
            output_id: crate::new_id(),
            kind,
            label: label.to_owned(),
            revision: Rc::new(RefCell::new(None)),
            closed: Rc::new(RefCell::new(false)),
        };

        {
            let held = transport.clone();

            listen(&transport.channel, move |message| {
                held.receive(message, on_state);
            });
        }

        post(
            &transport.channel,
            &SessionMessage::Hello {
                output_id: transport.output_id.clone(),
                kind,
                label: transport.label.clone(),
            },
        );
        post(
            &transport.channel,
            &SessionMessage::RequestState {
                output_id: transport.output_id.clone(),
            },
        );

        {
            let held = transport.clone();

            spawn_local(async move {
                loop {
                    gloo_timers::future::TimeoutFuture::new(HEARTBEAT_MS).await;

                    if *held.closed.borrow() {
                        return;
                    }

                    held.ack();
                }
            });
        }

        Some(transport)
    }

    pub fn receive(&self, message: SessionMessage, on_state: Callback<SessionState>) {
        let SessionMessage::State { state } = message else {
            return;
        };

        // Business rule 5 of the stage view: never move backwards on a late message.
        let applied = *self.revision.borrow();

        if applied.is_some_and(|held| state.revision < held) {
            return;
        }

        *self.revision.borrow_mut() = Some(state.revision);
        on_state.run(*state);
        self.ack();
    }

    /// Ask the control surface to send the current state again — after a failed render, say.
    pub fn request_state(&self) {
        *self.revision.borrow_mut() = None;

        post(
            &self.channel,
            &SessionMessage::RequestState {
                output_id: self.output_id.clone(),
            },
        );
    }

    pub fn request_advance(&self, delta: i64) {
        post(
            &self.channel,
            &SessionMessage::Advance {
                output_id: self.output_id.clone(),
                delta,
            },
        );
    }

    pub fn output_id(&self) -> &str {
        &self.output_id
    }

    pub fn close(&self) {
        *self.closed.borrow_mut() = true;

        post(
            &self.channel,
            &SessionMessage::Bye {
                output_id: self.output_id.clone(),
            },
        );

        self.channel.close();
    }

    fn ack(&self) {
        post(
            &self.channel,
            &SessionMessage::Ack {
                output_id: self.output_id.clone(),
                kind: self.kind,
                label: self.label.clone(),
                revision: (*self.revision.borrow()).unwrap_or(0),
            },
        );
    }
}
