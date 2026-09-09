//! The theme editor.
//!
//! A theme belongs to the workspace and syncs like any other record, so a band's screens agree
//! without anyone configuring a second laptop. A change made here applies to the running session
//! immediately; saving it to the workspace is a separate, deliberate act.

use aurum_core::present::session::{Align, BackgroundKind, Theme};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::json;
use wasm_bindgen::JsCast;
use web_sys::{File, HtmlInputElement};

use crate::app::use_workspace;
use crate::db::live::live_query;
use crate::db::records::{PresenterTheme, alive};

fn kind_of(value: &str) -> BackgroundKind {
    match value {
        "gradient" => BackgroundKind::Gradient,
        "image" => BackgroundKind::Image,
        _ => BackgroundKind::Color,
    }
}

fn align_of(value: &str) -> Align {
    if value == "left" {
        Align::Left
    } else {
        Align::Center
    }
}

/// A stored row read back as the theme it describes.
fn theme_of(row: &PresenterTheme) -> Theme {
    Theme {
        id: row.id.clone(),
        name: row.name.clone(),
        font_family: row.font_family.clone(),
        font_size_vh: row.font_size_vh,
        text_color: row.text_color.clone(),
        background_kind: kind_of(&row.background_kind),
        background_value: row.background_value.clone(),
        align: align_of(&row.align),
        safe_area_pct: row.safe_area_pct,
        show_section_labels: row.show_section_labels == 1,
    }
}

fn chosen_file(event: &web_sys::Event) -> Option<File> {
    event
        .target()
        .and_then(|target| target.dyn_into::<HtmlInputElement>().ok())
        .and_then(|input| input.files())
        .and_then(|files| files.get(0))
}

#[component]
pub fn ThemeDrawer(
    theme: Signal<Theme>,
    on_change: Callback<Theme>,
    on_close: Callback<()>,
) -> impl IntoView {
    let context = use_workspace();
    let can_edit = context.can_edit();
    let db = context.db;
    let uploading = RwSignal::new(None::<String>);

    let saved = live_query(&["presenter_themes"], move || {
        let db = db.get();

        async move {
            match db {
                Some(db) => alive(
                    db.all::<PresenterTheme>("presenter_themes")
                        .await
                        .unwrap_or_default(),
                ),
                None => Vec::new(),
            }
        }
    });

    let stored = Signal::derive(move || saved.get().unwrap_or_default());

    let save_to_workspace = move |_| {
        let (Some(engine), current) = (context.engine.get_untracked(), theme.get_untracked())
        else {
            return;
        };

        let rows = stored.get_untracked();
        let id = rows
            .iter()
            .find(|row| row.name == current.name)
            .map(|row| row.id.clone())
            .unwrap_or_else(crate::new_id);
        let first = rows.is_empty();

        spawn_local(async move {
            let _ = engine
                .record(
                    "presenter_themes",
                    &id,
                    "upsert",
                    json!({
                        "name": current.name,
                        "is_default": i64::from(first),
                        "font_family": current.font_family,
                        "font_size_vh": current.font_size_vh,
                        "text_color": current.text_color,
                        "background_kind": match current.background_kind {
                            BackgroundKind::Color => "color",
                            BackgroundKind::Gradient => "gradient",
                            BackgroundKind::Image => "image",
                        },
                        "background_value": current.background_value,
                        "align": match current.align {
                            Align::Left => "left",
                            Align::Center => "center",
                        },
                        "safe_area_pct": current.safe_area_pct,
                        "show_section_labels": i64::from(current.show_section_labels),
                    })
                    .as_object()
                    .cloned()
                    .unwrap_or_default(),
                )
                .await;
        });
    };

    view! {
        <div
            class="fixed inset-0 z-20 flex justify-end bg-slate-900/40"
            on:click=move |_| on_close.run(())
        >
            <div
                class="h-full w-80 overflow-auto bg-white p-4 text-sm shadow-xl dark:bg-slate-900"
                data-testid="theme-drawer"
                on:click=|event| event.stop_propagation()
            >
                <h2 class="mb-3 text-lg font-semibold">"Theme"</h2>

                <Show when=move || !stored.get().is_empty()>
                    <label class="mb-3 block">
                        <span class="text-slate-500">"Workspace themes"</span>
                        <select
                            class="w-full rounded border border-slate-300 bg-transparent px-2 py-1 dark:border-slate-700"
                            prop:value=move || theme.get().id
                            on:change=move |event| {
                                let wanted = event_target_value(&event);

                                if let Some(found) = stored
                                    .get_untracked()
                                    .iter()
                                    .find(|row| row.id == wanted)
                                {
                                    on_change.run(theme_of(found));
                                }
                            }
                        >
                            <option value=move || theme.get().id>{move || theme.get().name}</option>
                            <For each=move || stored.get() key=|row| row.id.clone() let:row>
                                <option value=row.id.clone()>{row.name.clone()}</option>
                            </For>
                        </select>
                    </label>
                </Show>

                <Field label="Name">
                    <input
                        class="w-full rounded border border-slate-300 px-2 py-1 dark:border-slate-700 dark:bg-slate-950"
                        prop:value=move || theme.get().name
                        on:input=move |event| {
                            on_change.run(Theme {
                                name: event_target_value(&event),
                                ..theme.get_untracked()
                            });
                        }
                    />
                </Field>

                <Field label="Text colour">
                    <input
                        type="color"
                        prop:value=move || theme.get().text_color
                        on:input=move |event| {
                            on_change.run(Theme {
                                text_color: event_target_value(&event),
                                ..theme.get_untracked()
                            });
                        }
                    />
                </Field>

                <Field label="Background">
                    <input
                        type="color"
                        prop:value=move || {
                            let held = theme.get();

                            if held.background_kind == BackgroundKind::Color {
                                held.background_value
                            } else {
                                "#000000".to_owned()
                            }
                        }
                        on:input=move |event| {
                            on_change.run(Theme {
                                background_kind: BackgroundKind::Color,
                                background_value: event_target_value(&event),
                                ..theme.get_untracked()
                            });
                        }
                    />
                </Field>

                <Field
                    label="Background image"
                    hint="Optional; the colour shows through if it is not on this device"
                >
                    <input
                        type="file"
                        accept="image/png,image/jpeg,image/webp,image/avif"
                        class="w-full text-xs"
                        on:change=move |event| {
                            let (Some(file), Some(db)) = (chosen_file(&event), db.get_untracked())
                            else {
                                return;
                            };

                            let api = context.api.clone();
                            let workspace_id = context.workspace.get_untracked().id;

                            uploading.set(Some("Uploading…".to_owned()));

                            spawn_local(async move {
                                match super::background::upload(&db, &api, &workspace_id, &file)
                                    .await
                                {
                                    Some(sha256) => {
                                        on_change.run(Theme {
                                            background_kind: BackgroundKind::Image,
                                            background_value: sha256,
                                            ..theme.get_untracked()
                                        });
                                        uploading.set(None);
                                    }

                                    None => uploading.set(Some(
                                        "The image could not be uploaded.".to_owned(),
                                    )),
                                }
                            });
                        }
                    />

                    <Show when=move || uploading.get().is_some()>
                        <p class="text-xs text-slate-500">{move || uploading.get()}</p>
                    </Show>

                    <Show when=move || theme.get().background_kind == BackgroundKind::Image>
                        <button
                            class="mt-1 text-xs underline"
                            on:click=move |_| {
                                on_change.run(Theme {
                                    background_kind: BackgroundKind::Color,
                                    background_value: "#000000".to_owned(),
                                    ..theme.get_untracked()
                                });
                            }
                        >
                            "Remove the image"
                        </button>
                    </Show>
                </Field>

                <label class="mb-3 block">
                    <span class="text-slate-500">
                        {move || format!("Maximum size — {}vh", theme.get().font_size_vh)}
                    </span>
                    <div>
                        <input
                            type="range"
                            min="4"
                            max="16"
                            step="0.5"
                            prop:value=move || theme.get().font_size_vh
                            on:input=move |event| {
                                on_change.run(Theme {
                                    font_size_vh: event_target_value(&event)
                                        .parse()
                                        .unwrap_or(8.0),
                                    ..theme.get_untracked()
                                });
                            }
                        />
                    </div>
                </label>

                <label class="mb-3 block">
                    <span class="text-slate-500">
                        {move || format!("Safe margin — {}%", theme.get().safe_area_pct)}
                    </span>
                    <div>
                        <input
                            type="range"
                            min="0"
                            max="15"
                            step="1"
                            prop:value=move || theme.get().safe_area_pct
                            on:input=move |event| {
                                on_change.run(Theme {
                                    safe_area_pct: event_target_value(&event)
                                        .parse()
                                        .unwrap_or(5.0),
                                    ..theme.get_untracked()
                                });
                            }
                        />
                    </div>
                </label>

                <Field label="Alignment">
                    <select
                        class="w-full rounded border border-slate-300 bg-transparent px-2 py-1 dark:border-slate-700"
                        prop:value=move || match theme.get().align {
                            Align::Left => "left",
                            Align::Center => "center",
                        }
                        on:change=move |event| {
                            on_change.run(Theme {
                                align: align_of(&event_target_value(&event)),
                                ..theme.get_untracked()
                            });
                        }
                    >
                        <option value="center">"Centre"</option>
                        <option value="left">"Left"</option>
                    </select>
                </Field>

                <label class="mb-3 flex items-center gap-2">
                    <input
                        type="checkbox"
                        prop:checked=move || theme.get().show_section_labels
                        on:change=move |event| {
                            on_change.run(Theme {
                                show_section_labels: event_target_checked(&event),
                                ..theme.get_untracked()
                            });
                        }
                    />
                    "Show section labels on the audience screen"
                </label>

                <p class="mb-3 text-xs text-slate-500">
                    "Changing the size rebuilds the slides at the next session; the running \
                     session keeps the slides it started with, so nothing moves under the \
                     operator mid-service."
                </p>

                <div class="flex gap-2">
                    <Show when=move || can_edit>
                        <button
                            class="rounded bg-slate-900 px-3 py-2 text-white dark:bg-slate-100 dark:text-slate-900"
                            on:click=save_to_workspace
                        >
                            "Save to the workspace"
                        </button>
                    </Show>
                    <button class="underline" on:click=move |_| on_close.run(())>"Close"</button>
                </div>
            </div>
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
        <label class="mb-3 block">
            <span class="text-slate-500">{label}</span>
            {hint.map(|hint| view! { <span class="ml-2 text-xs text-slate-400">{hint}</span> })}
            <div>{children()}</div>
        </label>
    }
}

/// The workspace's default theme, or the built-in one before a band has made its own.
pub async fn workspace_theme(db: &crate::db::Database) -> Theme {
    let rows = alive(
        db.all::<PresenterTheme>("presenter_themes")
            .await
            .unwrap_or_default(),
    );

    rows.iter()
        .find(|row| row.is_default == 1)
        .or_else(|| rows.first())
        .map(theme_of)
        .unwrap_or_default()
}
