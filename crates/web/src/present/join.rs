//! Joining a running session from a phone or a tablet.
//!
//! A failed join never touches the session that is running (stage-view failure behaviour), and a
//! join that succeeds and then drops keeps the last slide on screen while it tries again — for
//! five minutes, with backoff, because a musician cannot debug a network mid-song.

use aurum_core::present::session::{
    OutputKind, SessionMessage, SessionState, is_valid_code, normalise_code,
};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_query_map;

use super::pairing::join;
use super::stage::StageScreen;
use super::transport::Send;
use crate::app::storage;

/// How long a dropped device keeps trying before it stops and waits to be told to try again.
const RETRY_FOR_MS: i64 = 5 * 60 * 1000;
const RETRY_EVERY_MS: u32 = 3000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Idle,
    Joining,
    Joined,
    /// Connected once, and not any more. The last slide stays up.
    Stale,
}

/// The last state this device saw, so a reopened tab paints instantly rather than blankly.
fn cached() -> Option<SessionState> {
    storage::read(storage::STAGE_LAST).and_then(|held| serde_json::from_str(&held).ok())
}

#[component]
pub fn JoinPage() -> impl IntoView {
    let params = use_query_map();
    let asked = params.read_untracked().get("code").unwrap_or_default();

    let code = RwSignal::new(normalise_code(&asked));
    let state = RwSignal::new(cached());
    let phase = RwSignal::new(Phase::Idle);
    let problem = RwSignal::new(None::<String>);

    let send = StoredValue::new_local(None::<Send>);
    let since = StoredValue::new(0_i64);
    let revision = StoredValue::new(None::<u64>);

    // The control surface counts an output as responding by its acks, so a paired device says so
    // on every state and once a second in between — the same heartbeat a window on the control
    // device sends.
    let ack = move || {
        send.with_value(|held| {
            if let Some(send) = held {
                send(&SessionMessage::Ack {
                    output_id: "paired".to_owned(),
                    kind: OutputKind::PairedStage,
                    label: "Paired device".to_owned(),
                    revision: revision.get_value().unwrap_or(0),
                });
            }
        });
    };

    spawn_local(async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(1000).await;

            if send.try_with_value(|held| held.is_some()) != Some(true) {
                // The screen is gone, or nothing is connected yet.
                if send.try_get_value().is_none() {
                    return;
                }

                continue;
            }

            ack();
        }
    });

    let attempt = move |value: String| {
        phase.set(Phase::Joining);
        problem.set(None);

        spawn_local(async move {
            let joined = join(
                &value,
                Callback::new(move |message: SessionMessage| {
                    // Business rule 5: a message that arrives out of order must not move the
                    // screen back.
                    let SessionMessage::State { state: next } = message else {
                        return;
                    };

                    if revision
                        .get_value()
                        .is_some_and(|held| next.revision < held)
                    {
                        return;
                    }

                    revision.set_value(Some(next.revision));

                    if let Ok(held) = serde_json::to_string(&*next) {
                        storage::write(storage::STAGE_LAST, &held);
                    }

                    // When the session ends this device goes back to the join screen rather than
                    // holding the last slide: the service is over, and the tablet is a tablet.
                    if next.ended {
                        storage::remove(storage::STAGE_LAST);
                        state.set(None);
                        phase.set(Phase::Idle);

                        return;
                    }

                    state.set(Some(*next));
                    phase.set(Phase::Joined);
                    ack();
                }),
                Callback::new(move |()| phase.set(Phase::Stale)),
            )
            .await;

            match joined {
                Ok(channel) => {
                    send.set_value(Some(channel));
                    phase.set(Phase::Joined);
                    since.set_value(crate::now_ms());
                }

                Err(why) => {
                    phase.set(Phase::Idle);
                    problem.set(Some(why.to_string()));
                }
            }
        });
    };

    // A link with the code in it joins without anybody typing anything.
    if is_valid_code(&asked) {
        attempt(normalise_code(&asked));
    }

    // Rejoin with backoff while the session is presumably still running.
    spawn_local(async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(RETRY_EVERY_MS).await;

            let Some(Phase::Stale) = phase.try_get() else {
                if phase.try_get().is_none() {
                    return;
                }

                continue;
            };

            let started = since.get_value();

            if started != 0 && crate::now_ms() - started > RETRY_FOR_MS {
                continue;
            }

            attempt(code.get_untracked());
        }
    });

    let showing = Signal::derive(move || matches!(phase.get(), Phase::Joined | Phase::Stale));

    view! {
        <Show
            when=move || showing.get()
            fallback=move || view! {
                <div class="mx-auto max-w-sm p-6">
                    <h2 class="mb-2 text-xl font-semibold" data-testid="screen-title">
                        "Join a session"
                    </h2>
                    <p class="mb-4 text-sm text-ink-3">
                        "Enter the six characters shown on the control screen. Both devices need \
                         to be on the same network and signed into this workspace."
                    </p>

                    <form on:submit=move |event| {
                        event.prevent_default();

                        let value = code.get_untracked();

                        if is_valid_code(&value) {
                            attempt(normalise_code(&value));
                        }
                    }>
                        <input
                            class="mb-3 w-full rounded-md border border-line-strong px-3 py-3 text-center font-mono text-2xl tracking-widest uppercase"
                            data-testid="join-code"
                            placeholder="4KJ9QP"
                            maxlength="6"
                            autofocus
                            prop:value=move || code.get()
                            on:input=move |event| {
                                code.set(normalise_code(&event_target_value(&event)));
                            }
                        />

                        <button
                            class="w-full rounded-md bg-accent py-2 text-on-accent disabled:opacity-40"
                            prop:disabled=move || {
                                !is_valid_code(&code.get()) || phase.get() == Phase::Joining
                            }
                        >
                            {move || if phase.get() == Phase::Joining { "Joining…" } else { "Join" }}
                        </button>
                    </form>

                    <Show when=move || problem.get().is_some()>
                        <p
                            class="mt-3 text-sm text-live-ink"
                            data-testid="join-problem"
                        >
                            {move || problem.get()}
                        </p>
                    </Show>
                </div>
            }
        >
            <StageScreen
                state=state.into()
                stale=Signal::derive(move || phase.get() == Phase::Stale)
                on_advance=Callback::new(move |delta: i64| {
                    send.with_value(|held| {
                        if let Some(send) = held {
                            send(&SessionMessage::Advance {
                                output_id: "paired".to_owned(),
                                delta,
                            });
                        }
                    });
                })
            />
        </Show>
    }
}
