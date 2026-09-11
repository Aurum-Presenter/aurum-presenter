//! What the sync is doing, for the one moment a musician cares: when something has not gone
//! through.
//!
//! Nothing here blocks anything. The panel exists so that "pending" has an answer, and so that a
//! parked operation — one the server refused — can be retried or thrown away deliberately rather
//! than sitting in a queue forever.

use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::app::use_workspace;
use crate::db::records::OutboxOp;
use crate::sync::engine::Status;

/// How often the panel re-reads the queue while it is open.
const REFRESH_MS: u32 = 2_000;

fn ago(at: Option<&str>, now_ms: i64) -> String {
    let Some(at) = at.and_then(aurum_core::time::parse) else {
        return "never".to_owned();
    };

    let seconds = (now_ms - at) / 1000;

    if seconds < 60 {
        "just now".to_owned()
    } else if seconds < 3600 {
        format!("{} minutes ago", (seconds as f64 / 60.0).round() as i64)
    } else {
        format!("{} hours ago", (seconds as f64 / 3600.0).round() as i64)
    }
}

fn changes(count: usize) -> String {
    match count {
        1 => "1 change".to_owned(),
        many => format!("{many} changes"),
    }
}

#[component]
pub fn SyncPanel(on_close: Callback<()>) -> impl IntoView {
    let context = use_workspace();
    let online = context.online;
    let engine = context.engine;
    let db = context.db;

    let status = RwSignal::new(Status::default());
    let parked = RwSignal::new(Vec::<OutboxOp>::new());
    let uploads = RwSignal::new(0usize);
    let open = RwSignal::new(true);

    let refresh = move || {
        spawn_local(async move {
            if let Some(engine) = engine.get_untracked() {
                status.set(engine.status().await);
                parked.set(engine.parked().await.unwrap_or_default());
            }

            if let Some(db) = db.get_untracked() {
                uploads.set(
                    db.all::<serde_json::Value>("uploads")
                        .await
                        .map(|rows| rows.len())
                        .unwrap_or(0),
                );
            }
        });
    };

    // A poll rather than a subscription: the numbers come from a queue this window may not own,
    // and two seconds is soon enough for a panel somebody is watching.
    Effect::new(move |_| {
        refresh();

        spawn_local(async move {
            while open.get_untracked() {
                TimeoutFuture::new(REFRESH_MS).await;

                if open.get_untracked() {
                    refresh();
                }
            }
        });
    });

    on_cleanup(move || open.set(false));

    let close = move || {
        open.set(false);
        on_close.run(());
    };

    let sync_now = move |_| {
        spawn_local(async move {
            if let Some(engine) = engine.get_untracked() {
                let _ = engine.sync().await;
            }

            refresh();
        });
    };

    view! {
        <div
            class="fixed inset-0 z-30 flex justify-end bg-slate-900/40"
            data-testid="sync-panel"
            on:click=move |_| close()
        >
            <div
                class="h-full w-96 overflow-auto bg-white p-4 text-sm dark:bg-slate-900"
                on:click=|event| event.stop_propagation()
            >
                <h2 class="mb-3 text-lg font-semibold">"Sync"</h2>

                <dl class="mb-4 space-y-1">
                    <Row
                        label="Connection"
                        value=Signal::derive(move || {
                            if online.get() {
                                "online".to_owned()
                            } else {
                                "offline — everything still works".to_owned()
                            }
                        })
                    />
                    <Row
                        label="Waiting to send"
                        value=Signal::derive(move || changes(status.get().pending))
                    />
                    <Row
                        label="Files waiting"
                        value=Signal::derive(move || uploads.get().to_string())
                    />
                    <Row
                        label="Last received"
                        value=Signal::derive(move || {
                            ago(status.get().last_pull.as_deref(), crate::now_ms())
                        })
                    />
                    <Row
                        label="Last sent"
                        value=Signal::derive(move || {
                            ago(status.get().last_push.as_deref(), crate::now_ms())
                        })
                    />
                </dl>

                <div class="mb-4 flex gap-3">
                    <button
                        class="rounded bg-slate-900 px-3 py-2 text-white dark:bg-slate-100 dark:text-slate-900"
                        data-testid="sync-now"
                        on:click=sync_now
                    >
                        "Sync now"
                    </button>
                    <A
                        href="/settings/sync/conflicts"
                        attr:class="self-center underline"
                        on:click=move |_| close()
                    >
                        "Conflicts"
                    </A>
                    <A
                        href="/settings/storage"
                        attr:class="self-center underline"
                        on:click=move |_| close()
                    >
                        "Storage"
                    </A>
                </div>

                <Show when=move || !parked.get().is_empty()>
                    <section>
                        <h3 class="mb-1 font-semibold text-amber-700 dark:text-amber-400">
                            {move || {
                                format!("{} the server refused", changes(parked.get().len()))
                            }}
                        </h3>
                        <p class="mb-2 text-xs text-slate-500">
                            "These are kept, not lost. Fix what caused them — a permission, a \
                             record someone deleted — and retry, or discard the change if it is \
                             no longer wanted."
                        </p>

                        <ul class="space-y-2">
                            <For
                                each=move || parked.get()
                                key=|op| op.seq.unwrap_or_default()
                                let:op
                            >
                                <Parked op=op on_done=Callback::new(move |()| refresh()) />
                            </For>
                        </ul>
                    </section>
                </Show>

                <button class="mt-4 underline" on:click=move |_| close()>"Close"</button>
            </div>
        </div>
    }
}

/// One refused change. Its own component so the two handlers stay `Fn`.
#[component]
fn Parked(op: OutboxOp, on_done: Callback<()>) -> impl IntoView {
    let engine = use_workspace().engine;
    let seq = op.seq.unwrap_or_default();

    let act = move |retry: bool| {
        spawn_local(async move {
            if let Some(engine) = engine.get_untracked() {
                let _ = if retry {
                    engine.retry(seq).await
                } else {
                    engine.discard(seq).await
                };
            }

            on_done.run(());
        });
    };

    view! {
        <li class="rounded border border-slate-200 p-2 dark:border-slate-800">
            <p class="font-mono text-xs">{format!("{} {}", op.op, op.table)}</p>
            <p class="text-xs text-slate-500">
                {op.last_error.clone().unwrap_or_else(|| "refused".to_owned())}
            </p>
            <p class="mt-1 flex gap-3">
                <button class="underline" on:click=move |_| act(true)>"Retry"</button>
                <button
                    class="underline text-red-700 dark:text-red-400"
                    on:click=move |_| act(false)
                >
                    "Discard"
                </button>
            </p>
        </li>
    }
}

#[component]
fn Row(label: &'static str, value: Signal<String>) -> impl IntoView {
    view! {
        <div class="flex gap-2">
            <dt class="w-36 text-slate-500">{label}</dt>
            <dd>{move || value.get()}</dd>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_788_912_000_000;

    #[test]
    fn a_sync_that_never_happened_says_so() {
        assert_eq!(ago(None, NOW), "never");
        assert_eq!(ago(Some("not a date"), NOW), "never");
    }

    #[test]
    fn recent_sync_times_are_read_in_the_units_a_person_would_use() {
        let at = |ms: i64| aurum_core::time::format(NOW - ms);

        assert_eq!(ago(Some(&at(5_000)), NOW), "just now");
        assert_eq!(ago(Some(&at(300_000)), NOW), "5 minutes ago");
        assert_eq!(ago(Some(&at(7_200_000)), NOW), "2 hours ago");
    }
}
