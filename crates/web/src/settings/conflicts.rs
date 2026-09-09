//! The conflict review.
//!
//! Last-writer-wins is how the sync resolves a field two people changed at once, but the value
//! that lost is never destroyed: the server keeps it, every device can see it, and it can be
//! written back as a new edit. That is the difference between a merge rule and data loss.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::app::use_workspace;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct ServerConflict {
    id: String,
    table_name: String,
    record_id: String,
    field: String,
    losing_value: Option<String>,
    #[serde(default)]
    losing_user: Option<String>,
    at: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
struct Conflicts {
    conflicts: Vec<ServerConflict>,
}

/// One conflict, with the value that is there now beside the one that was overwritten.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Reviewable {
    conflict: ServerConflict,
    now: Option<String>,
}

#[component]
pub fn ConflictsPage() -> impl IntoView {
    let context = use_workspace();
    let can_edit = context.can_edit();
    let db = context.db;
    let workspace = context.workspace;
    let api = context.api.clone();

    let restored = RwSignal::new(Vec::<String>::new());
    let problem = RwSignal::new(None::<String>);

    let rows = LocalResource::new(move || {
        let (api, id, db) = (api.clone(), workspace.get().id, db.get());

        async move {
            let Ok(answer) = api
                .get::<Conflicts>(&format!("/workspaces/{id}/sync/conflicts"))
                .await
            else {
                problem.set(Some(
                    "The conflict list could not be fetched. It needs a connection.".to_owned(),
                ));

                return None;
            };

            problem.set(None);

            let mut reviewable = Vec::new();

            for conflict in answer.conflicts {
                // The value that won is whatever is in the local copy now, which is what the
                // reviewer is comparing against.
                let now = match &db {
                    Some(db) => db
                        .get::<Map<String, Value>>(&conflict.table_name, &conflict.record_id)
                        .await
                        .ok()
                        .flatten()
                        .and_then(|row| row.get(&conflict.field).cloned())
                        .map(|value| match value {
                            Value::String(text) => text,
                            Value::Null => String::new(),
                            other => other.to_string(),
                        }),
                    None => None,
                };

                reviewable.push(Reviewable { conflict, now });
            }

            Some(reviewable)
        }
    });

    // Restoring writes a new edit rather than rewinding anything, so the row leaves the list
    // here and the server's record of the conflict stays where it is.
    let open = Signal::derive(move || {
        let gone = restored.get();

        rows.get()
            .flatten()
            .unwrap_or_default()
            .into_iter()
            .filter(|row| !gone.contains(&row.conflict.id))
            .collect::<Vec<_>>()
    });

    let restore = move |conflict: ServerConflict| {
        let Some(engine) = context.engine.get_untracked() else {
            return;
        };

        spawn_local(async move {
            let _ = engine
                .record(
                    &conflict.table_name,
                    &conflict.record_id,
                    "upsert",
                    json!({ conflict.field.clone(): conflict.losing_value })
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                )
                .await;

            restored.update(|gone| gone.push(conflict.id));
        });
    };

    view! {
        <div class="mx-auto max-w-3xl p-4">
            <A href="/library" attr:class="text-sm underline">"← Library"</A>
            <h2 class="mb-1 mt-3 text-2xl font-semibold" data-testid="screen-title">"Conflicts"</h2>
            <p class="mb-4 text-sm text-slate-500">
                "When two people change the same field before either has synced, the later edit \
                 wins and the earlier one is kept here. Nothing is deleted; the older value can \
                 be put back."
            </p>

            <Show when=move || problem.get().is_some()>
                <p class="text-sm text-amber-700 dark:text-amber-400">{move || problem.get()}</p>
            </Show>

            {move || match (rows.get().is_none(), open.get()) {
                (true, _) => view! { <p class="text-sm text-slate-500">"Loading…"</p> }.into_any(),

                (false, found) if found.is_empty() => view! {
                    <p class="text-sm text-slate-500" data-testid="no-conflicts">
                        "Nothing has been overwritten."
                    </p>
                }
                .into_any(),

                (false, found) => view! {
                    <ul class="space-y-3" data-testid="conflicts">
                        {found
                            .into_iter()
                            .map(|row| {
                                let conflict = row.conflict.clone();

                                view! {
                                    <li class="rounded border border-slate-200 p-3 text-sm dark:border-slate-800">
                                        <p class="font-medium">
                                            {format!(
                                                "{}.{}",
                                                conflict.table_name,
                                                conflict.field,
                                            )}
                                            <span class="ml-2 text-xs text-slate-500">
                                                {conflict.at.clone()}
                                            </span>
                                        </p>

                                        <div class="mt-2 grid gap-2 md:grid-cols-2">
                                            <Held
                                                label="Now"
                                                value=row.now.clone().unwrap_or_else(|| "—".to_owned())
                                            />
                                            <Held
                                                label="Overwritten"
                                                value=conflict
                                                    .losing_value
                                                    .clone()
                                                    .unwrap_or_else(|| "—".to_owned())
                                            />
                                        </div>

                                        <Show when=move || can_edit>
                                            <button
                                                class="mt-2 underline"
                                                data-testid="restore-value"
                                                on:click={
                                                    let conflict = conflict.clone();

                                                    move |_| restore(conflict.clone())
                                                }
                                            >
                                                "Put the overwritten value back"
                                            </button>
                                        </Show>
                                    </li>
                                }
                            })
                            .collect_view()}
                    </ul>
                }
                .into_any(),
            }}
        </div>
    }
}

#[component]
fn Held(label: &'static str, value: String) -> impl IntoView {
    view! {
        <div>
            <p class="text-xs uppercase tracking-widest text-slate-500">{label}</p>
            <pre class="max-h-40 overflow-auto whitespace-pre-wrap rounded bg-slate-50 p-2 text-xs dark:bg-slate-800">
                {value}
            </pre>
        </div>
    }
}
