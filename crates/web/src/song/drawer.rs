//! The song's metadata, as a drawer over the chart.
//!
//! Validation is immediate and advisory: it names what is wrong, and refuses only the things
//! that would make the record meaningless — an empty title, a tempo no instrument plays.

use aurum_core::library::validation::{format_duration, parse_duration};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::app::use_workspace;
use crate::db::records::Song;
use crate::library::repository::{SongInput, list_of, validate};

/// The typed form. It is not `SongInput` because three of its fields are text the user is still
/// in the middle of typing — a length of `4:` is not yet a number of seconds.
#[derive(Clone, Debug, Default, PartialEq)]
struct Form {
    title: String,
    subtitle: String,
    artist: String,
    authors: String,
    ccli_number: String,
    copyright: String,
    notes: String,
    original_key: String,
    tempo: String,
    time_signature: String,
    duration: String,
    tags: String,
    alt_titles: String,
}

fn optional(value: &str) -> Option<String> {
    let trimmed = value.trim();

    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn split(text: &str) -> Vec<String> {
    text.split(',')
        .map(|part| part.trim().to_owned())
        .filter(|part| !part.is_empty())
        .collect()
}

fn form_of(song: &Song) -> Form {
    Form {
        title: song.title.clone(),
        subtitle: song.subtitle.clone().unwrap_or_default(),
        artist: song.artist.clone().unwrap_or_default(),
        authors: song.authors.clone().unwrap_or_default(),
        ccli_number: song.ccli_number.clone().unwrap_or_default(),
        copyright: song.copyright.clone().unwrap_or_default(),
        notes: song.notes.clone().unwrap_or_default(),
        original_key: song.original_key.clone().unwrap_or_default(),
        tempo: song
            .tempo
            .map(|value| value.to_string())
            .unwrap_or_default(),
        time_signature: song.time_signature.clone().unwrap_or_default(),
        duration: format_duration(song.duration_sec),
        tags: list_of(song.tags.as_deref()).join(", "),
        alt_titles: list_of(song.alt_titles.as_deref()).join(", "),
    }
}

#[component]
pub fn SongMetadataDrawer(song: Song, on_close: Callback<()>) -> impl IntoView {
    let context = use_workspace();
    let song_id = song.id.clone();
    let folder_id = song.folder_id.clone();
    let form = RwSignal::new(form_of(&song));
    let problems = RwSignal::new(Vec::<String>::new());

    let submit = move |event: web_sys::SubmitEvent| {
        event.prevent_default();

        let current = form.get_untracked();
        let duration_typed = !current.duration.trim().is_empty();
        let duration_sec = parse_duration(&current.duration);

        let input = SongInput {
            title: current.title.trim().to_owned(),
            folder_id: folder_id.clone(),
            artist: optional(&current.artist),
            authors: optional(&current.authors),
            subtitle: optional(&current.subtitle),
            ccli_number: optional(&current.ccli_number),
            copyright: optional(&current.copyright),
            notes: optional(&current.notes),
            original_key: optional(&current.original_key),
            // A tempo that is not a number is caught below by the same range check that catches
            // a year typed into the box.
            tempo: optional(&current.tempo).and_then(|value| value.parse().ok()),
            time_signature: optional(&current.time_signature),
            duration_sec,
            tags: split(&current.tags),
            alt_titles: split(&current.alt_titles),
        };

        let mut found = validate(&input);

        if duration_typed && duration_sec.is_none() {
            found.push("Length is mm:ss, for example 4:05.".to_owned());
        }

        if !found.is_empty() {
            problems.set(found);
            return;
        }

        let Some(library) = context.library() else {
            return;
        };

        let song_id = song_id.clone();

        spawn_local(async move {
            if library.update_song(&song_id, &input).await.is_ok() {
                on_close.run(());
            }
        });
    };

    view! {
        <div
            class="fixed inset-0 z-20 flex justify-end bg-black/50"
            on:click=move |_| on_close.run(())
        >
            <form
                class="h-full w-full max-w-md overflow-auto bg-surface p-4 shadow-xl"
                data-testid="song-details"
                on:click=|event| event.stop_propagation()
                on:submit=submit
            >
                <h2 class="mb-3 text-lg font-semibold">"Song details"</h2>

                <Show when=move || !problems.get().is_empty()>
                    <ul class="mb-3 rounded-md border border-live/50 bg-live/10 p-2 text-sm text-live-ink">
                        <For each=move || problems.get() key=|problem| problem.clone() let:problem>
                            <li>{problem}</li>
                        </For>
                    </ul>
                </Show>

                <Field label="Title">
                    <Text
                        value=Signal::derive(move || form.get().title)
                        on_change=Callback::new(move |value| {
                            form.update(|held| held.title = value)
                        })
                    />
                </Field>

                <Field label="Also known as" hint="Comma separated">
                    <Text
                        value=Signal::derive(move || form.get().alt_titles)
                        on_change=Callback::new(move |value| {
                            form.update(|held| held.alt_titles = value)
                        })
                    />
                </Field>

                <Field label="Subtitle">
                    <Text
                        value=Signal::derive(move || form.get().subtitle)
                        on_change=Callback::new(move |value| {
                            form.update(|held| held.subtitle = value)
                        })
                    />
                </Field>

                <Field label="Artist">
                    <Text
                        value=Signal::derive(move || form.get().artist)
                        on_change=Callback::new(move |value| {
                            form.update(|held| held.artist = value)
                        })
                    />
                </Field>

                <Field label="Author">
                    <Text
                        value=Signal::derive(move || form.get().authors)
                        on_change=Callback::new(move |value| {
                            form.update(|held| held.authors = value)
                        })
                    />
                </Field>

                <Field label="Original key">
                    <Text
                        value=Signal::derive(move || form.get().original_key)
                        on_change=Callback::new(move |value| {
                            form.update(|held| held.original_key = value)
                        })
                    />
                </Field>

                <div class="grid grid-cols-3 gap-2">
                    <Field label="Tempo">
                        <Text
                            value=Signal::derive(move || form.get().tempo)
                            on_change=Callback::new(move |value| {
                                form.update(|held| held.tempo = value)
                            })
                        />
                    </Field>

                    <Field label="Time">
                        <Text
                            value=Signal::derive(move || form.get().time_signature)
                            on_change=Callback::new(move |value| {
                                form.update(|held| held.time_signature = value)
                            })
                        />
                    </Field>

                    <Field label="Length" hint="mm:ss">
                        <Text
                            value=Signal::derive(move || form.get().duration)
                            on_change=Callback::new(move |value| {
                                form.update(|held| held.duration = value)
                            })
                        />
                    </Field>
                </div>

                <Field label="Tags" hint="Comma separated">
                    <Text
                        value=Signal::derive(move || form.get().tags)
                        on_change=Callback::new(move |value| {
                            form.update(|held| held.tags = value)
                        })
                    />
                </Field>

                <Field label="CCLI">
                    <Text
                        value=Signal::derive(move || form.get().ccli_number)
                        on_change=Callback::new(move |value| {
                            form.update(|held| held.ccli_number = value)
                        })
                    />
                </Field>

                <Field label="Copyright">
                    <Text
                        value=Signal::derive(move || form.get().copyright)
                        on_change=Callback::new(move |value| {
                            form.update(|held| held.copyright = value)
                        })
                    />
                </Field>

                <Field label="Notes" hint="Never shown on the audience screen">
                    <textarea
                        class="w-full rounded-md border border-line-strong px-2 py-1"
                        rows="3"
                        prop:value=move || form.get().notes
                        on:input=move |event| {
                            let value = event_target_value(&event);

                            form.update(|held| held.notes = value);
                        }
                    />
                </Field>

                <div class="mt-4 flex gap-2">
                    <button class="rounded-md bg-accent px-4 py-2 text-sm text-on-accent">
                        "Save"
                    </button>
                    <button
                        type="button"
                        class="text-sm text-ink-3 hover:text-ink underline-offset-2 hover:underline"
                        on:click=move |_| on_close.run(())
                    >
                        "Cancel"
                    </button>
                </div>
            </form>
        </div>
    }
}

#[component]
fn Field(
    label: &'static str,
    #[prop(optional)] hint: Option<&'static str>,
    children: Children,
) -> impl IntoView {
    view! {
        <label class="mb-2 block text-sm">
            <span class="text-ink-3">{label}</span>
            {hint.map(|hint| view! { <span class="ml-2 text-xs text-ink-4">{hint}</span> })}
            {children()}
        </label>
    }
}

#[component]
fn Text(value: Signal<String>, on_change: Callback<String>) -> impl IntoView {
    view! {
        <input
            class="w-full rounded-md border border-line-strong px-2 py-1"
            prop:value=move || value.get()
            on:input=move |event| on_change.run(event_target_value(&event))
        />
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_field_is_no_value_rather_than_an_empty_one() {
        assert_eq!(optional("  "), None);
        assert_eq!(optional(" Hillsong "), Some("Hillsong".to_owned()));
    }

    #[test]
    fn a_comma_list_drops_the_spaces_and_the_gaps() {
        assert_eq!(split(" advent , , christmas "), vec!["advent", "christmas"]);
        assert!(split("   ").is_empty());
    }

    #[test]
    fn the_form_opens_on_what_the_song_already_says() {
        let song = Song {
            title: "Cornerstone".to_owned(),
            tempo: Some(72),
            duration_sec: Some(245),
            tags: Some(r#"["advent","modern"]"#.to_owned()),
            ..Song::default()
        };

        let form = form_of(&song);

        assert_eq!(form.title, "Cornerstone");
        assert_eq!(form.tempo, "72");
        assert_eq!(form.duration, "4:05");
        assert_eq!(form.tags, "advent, modern");
        // A field the song does not carry opens empty, not as the word "None".
        assert_eq!(form.artist, "");
    }
}
