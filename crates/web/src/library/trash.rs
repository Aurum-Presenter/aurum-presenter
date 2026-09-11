//! Trash.
//!
//! A deleted song is a tombstone, not a hole: it replicates to every device, and for thirty days
//! it can come back on any of them (business rule 3).

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::app::use_workspace;
use crate::db::live::live_query;
use crate::db::records::Song;

/// How long a deleted song is kept before the server's purge takes it for good.
const KEEP_DAYS: i64 = 30;

fn days_left(deleted_at: Option<&str>, now_ms: i64) -> String {
    let Some(at) = deleted_at.and_then(aurum_core::time::parse) else {
        return String::new();
    };

    let elapsed = (now_ms - at) as f64 / 86_400_000.0;
    let left = ((KEEP_DAYS as f64 - elapsed).ceil() as i64).max(0);

    match left {
        0 => "due to be purged".to_owned(),
        1 => "1 day left".to_owned(),
        many => format!("{many} days left"),
    }
}

#[component]
pub fn TrashPage() -> impl IntoView {
    let context = use_workspace();
    let can_edit = context.can_edit();
    let db = context.db;

    let deleted = live_query(&["songs"], move || {
        let db = db.get();

        async move {
            let Some(db) = db else {
                return Vec::new();
            };

            let mut rows: Vec<Song> = db
                .all::<Song>("songs")
                .await
                .unwrap_or_default()
                .into_iter()
                .filter(|song| song.sync.deleted_at.is_some())
                .collect();

            // Most recently deleted first: that is the one somebody is looking for.
            rows.sort_by(|left, right| right.sync.deleted_at.cmp(&left.sync.deleted_at));

            rows
        }
    });

    let rows = Signal::derive(move || deleted.get().unwrap_or_default());

    view! {
        <div class="mx-auto max-w-3xl p-4">
            <A href="/library" attr:class="text-sm underline">"← Library"</A>
            <h2 class="mb-1 mt-3 text-2xl font-semibold" data-testid="screen-title">"Trash"</h2>
            <p class="mb-4 text-sm text-slate-500">
                {format!(
                    "Deleted songs are kept for {KEEP_DAYS} days and can be restored on any device.",
                )}
            </p>

            <Show
                when=move || !rows.get().is_empty()
                fallback=|| view! {
                    <p class="text-sm text-slate-500">"Nothing has been deleted."</p>
                }
            >
                <ul class="divide-y divide-slate-200 dark:divide-slate-800" data-testid="trash">
                    <For each=move || rows.get() key=|song| song.id.clone() let:song>
                        <Deleted song can_edit />
                    </For>
                </ul>
            </Show>
        </div>
    }
}

#[component]
fn Deleted(song: Song, can_edit: bool) -> impl IntoView {
    let library = StoredValue::new(use_workspace().library());
    let id = StoredValue::new(song.id.clone());

    view! {
        <li class="flex items-center gap-3 py-2">
            <span class="font-medium">{song.title.clone()}</span>
            <span class="text-xs text-slate-500">
                {days_left(song.sync.deleted_at.as_deref(), crate::now_ms())}
            </span>

            <Show when=move || can_edit>
                <button
                    class="ml-auto text-sm underline"
                    data-testid="restore-song"
                    on:click=move |_| {
                        let (Some(library), id) = (library.get_value(), id.get_value()) else {
                            return;
                        };

                        spawn_local(async move {
                            let _ = library.restore_song(&id).await;
                        });
                    }
                >
                    "Restore"
                </button>
            </Show>
        </li>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_788_912_000_000;

    fn ago(days: i64) -> String {
        aurum_core::time::format(NOW - days * 86_400_000)
    }

    #[test]
    fn a_song_deleted_today_has_the_whole_window() {
        assert_eq!(days_left(Some(&ago(0)), NOW), "30 days left");
    }

    #[test]
    fn the_last_day_reads_as_one_day() {
        assert_eq!(days_left(Some(&ago(29)), NOW), "1 day left");
    }

    /// Past the window it is honest rather than silent: the purge has not run yet, and it will.
    #[test]
    fn past_the_window_it_says_so() {
        assert_eq!(days_left(Some(&ago(31)), NOW), "due to be purged");
    }

    #[test]
    fn a_song_that_is_not_deleted_says_nothing() {
        assert_eq!(days_left(None, NOW), "");
    }
}
