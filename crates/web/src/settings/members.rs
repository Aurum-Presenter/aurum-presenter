//! Who is in this workspace, and who has been asked.
//!
//! Roles are read from the membership row on the server; this page is a view of that, and for
//! anyone who is not an owner it is read-only — the controls are hidden rather than disabled,
//! because a disabled button is a promise the app cannot keep.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map};

use crate::api::workspace::{Invite, Member};
use crate::app::use_workspace;

#[component]
pub fn MembersPage() -> impl IntoView {
    let context = use_workspace();
    let api = StoredValue::new(context.api.clone());
    let workspace = context.workspace;
    let me = context.me;
    let owner = Signal::derive(move || workspace.get().role == "owner");

    let people = RwSignal::new(Vec::<Member>::new());
    let pending = RwSignal::new(Vec::<Invite>::new());
    let email = RwSignal::new(String::new());
    let role = RwSignal::new("editor".to_owned());
    let link = RwSignal::new(None::<String>);
    let problem = RwSignal::new(None::<String>);

    // Read once per workspace: membership is not something a screen should poll.
    Effect::new(move |_| {
        let (api, id, is_owner) = (api.get_value(), workspace.get().id, owner.get());

        problem.set(None);

        spawn_local(async move {
            match api.members(&id).await {
                Ok(found) => people.set(found.members),
                Err(_) => problem.set(Some("The member list needs a connection.".to_owned())),
            }

            if is_owner && let Ok(found) = api.invites(&id).await {
                pending.set(found.invites);
            }
        });
    });

    let invite = move |event: web_sys::SubmitEvent| {
        event.prevent_default();

        let (api, id, address, chosen) = (
            api.get_value(),
            workspace.get_untracked().id,
            email.get_untracked(),
            role.get_untracked(),
        );

        problem.set(None);

        spawn_local(async move {
            match api.invite(&id, &address, &chosen).await {
                Ok(answer) => {
                    pending.set(answer.pending);
                    link.set(answer.link);
                    email.set(String::new());
                }
                Err(why) => problem.set(Some(why.to_string())),
            }
        });
    };

    view! {
        <div class="mx-auto max-w-3xl p-4">
            <A href="/library" attr:class="text-sm underline">"← Library"</A>
            <h2 class="mb-1 mt-3 text-2xl font-semibold" data-testid="screen-title">
                {move || workspace.get().name}
            </h2>
            <p class="mb-4 text-sm text-slate-500">
                "Owners manage members. Editors change songs, charts and sets. Viewers read \
                 everything and keep their own keys, capos and notes."
            </p>

            <Show when=move || problem.get().is_some()>
                <p class="mb-3 rounded border border-amber-300 bg-amber-50 p-2 text-sm text-amber-900">
                    {move || problem.get()}
                </p>
            </Show>

            <ul class="mb-6 divide-y divide-slate-200 dark:divide-slate-800" data-testid="members">
                {move || people
                    .get()
                    .into_iter()
                    .map(|member| {
                        let user_id = StoredValue::new(member.user_id.clone());
                        let you = member.user_id == me.get().id;

                        view! {
                            <li class="flex flex-wrap items-center gap-3 py-2 text-sm">
                                <span class="font-medium">{member.display_name.clone()}</span>
                                <span class="text-slate-500">{member.email.clone()}</span>
                                {you.then(|| view! {
                                    <span class="text-xs text-slate-400">"you"</span>
                                })}

                                <Show
                                    when=move || owner.get()
                                    fallback={
                                        let role = member.role.clone();

                                        move || view! {
                                            <span class="ml-auto text-slate-500">{role.clone()}</span>
                                        }
                                    }
                                >
                                    <span class="ml-auto flex items-center gap-3">
                                        <select
                                            class="rounded border border-slate-300 bg-transparent px-2 py-1 dark:border-slate-700"
                                            prop:value=member.role.clone()
                                            on:change=move |event| {
                                                let (api, id, user, next) = (
                                                    api.get_value(),
                                                    workspace.get_untracked().id,
                                                    user_id.get_value(),
                                                    event_target_value(&event),
                                                );

                                                spawn_local(async move {
                                                    match api.set_role(&id, &user, &next).await {
                                                        Ok(found) => people.set(found.members),
                                                        Err(why) => {
                                                            problem.set(Some(why.to_string()))
                                                        }
                                                    }
                                                });
                                            }
                                        >
                                            <option value="owner">"owner"</option>
                                            <option value="editor">"editor"</option>
                                            <option value="viewer">"viewer"</option>
                                        </select>

                                        <button
                                            class="underline text-red-700 dark:text-red-400"
                                            on:click=move |_| {
                                                let (api, id, user) = (
                                                    api.get_value(),
                                                    workspace.get_untracked().id,
                                                    user_id.get_value(),
                                                );

                                                spawn_local(async move {
                                                    match api.remove_member(&id, &user).await {
                                                        Ok(found) => people.set(found.members),
                                                        Err(why) => {
                                                            problem.set(Some(why.to_string()))
                                                        }
                                                    }
                                                });
                                            }
                                        >
                                            "Remove"
                                        </button>
                                    </span>
                                </Show>
                            </li>
                        }
                    })
                    .collect_view()}

                <Show when=move || people.get().is_empty()>
                    <li class="py-2 text-sm text-slate-500">"Nobody else is here yet."</li>
                </Show>
            </ul>

            <Show when=move || owner.get()>
                <section>
                    <h3 class="mb-2 font-semibold">"Invite someone"</h3>

                    <form class="mb-3 flex flex-wrap gap-2" on:submit=invite>
                        <input
                            class="min-w-56 flex-1 rounded border border-slate-300 px-3 py-2 text-sm dark:border-slate-700 dark:bg-slate-900"
                            type="email"
                            placeholder="their email"
                            data-testid="invite-email"
                            prop:value=move || email.get()
                            on:input=move |event| email.set(event_target_value(&event))
                        />

                        <select
                            class="rounded border border-slate-300 bg-transparent px-2 py-2 text-sm dark:border-slate-700"
                            prop:value=move || role.get()
                            on:change=move |event| role.set(event_target_value(&event))
                        >
                            <option value="editor">"editor"</option>
                            <option value="viewer">"viewer"</option>
                        </select>

                        <button class="rounded bg-slate-900 px-4 py-2 text-sm text-white dark:bg-slate-100 dark:text-slate-900">
                            "Send invitation"
                        </button>
                    </form>

                    <p class="mb-3 text-xs text-slate-500">
                        "Ownership is granted after someone has joined and has two-factor \
                         authentication on their account — that is why it is not in this list."
                    </p>

                    <Show when=move || link.get().is_some()>
                        <p
                            class="mb-3 rounded border border-sky-300 bg-sky-50 p-2 text-sm text-sky-900"
                            data-testid="invite-link"
                        >
                            "Invitation sent. You can also hand it over directly: "
                            <span class="break-all font-mono text-xs">{move || link.get()}</span>
                        </p>
                    </Show>

                    <Show when=move || !pending.get().is_empty()>
                        <ul class="divide-y divide-slate-200 text-sm dark:divide-slate-800">
                            {move || pending
                                .get()
                                .into_iter()
                                .map(|held| {
                                    let invite_id = StoredValue::new(held.id.clone());

                                    view! {
                                        <li class="flex items-center gap-3 py-2">
                                            <span>{held.email.clone()}</span>
                                            <span class="text-slate-500">{held.role.clone()}</span>
                                            <span class="text-xs text-slate-400">
                                                {format!(
                                                    "expires {}",
                                                    held.expires_at.chars().take(10).collect::<String>(),
                                                )}
                                            </span>

                                            {held.not_sent.then(|| view! {
                                                <span
                                                    class="text-xs text-amber-700 dark:text-amber-400"
                                                    title="The mail server refused it five times"
                                                >
                                                    "not sent"
                                                </span>
                                            })}

                                            <button
                                                class="ml-auto underline"
                                                on:click=move |_| {
                                                    let (api, id, held) = (
                                                        api.get_value(),
                                                        workspace.get_untracked().id,
                                                        invite_id.get_value(),
                                                    );

                                                    spawn_local(async move {
                                                        if let Ok(found) =
                                                            api.revoke_invite(&id, &held).await
                                                        {
                                                            pending.set(found.invites);
                                                        }
                                                    });
                                                }
                                            >
                                                "Withdraw"
                                            </button>
                                        </li>
                                    }
                                })
                                .collect_view()}
                        </ul>
                    </Show>
                </section>
            </Show>
        </div>
    }
}

/// Accepting an invitation.
///
/// The link says which workspace and which address it was sent to before anything happens, so
/// somebody signed in as the wrong account can see that before they wonder why it failed.
#[component]
pub fn InvitePage() -> impl IntoView {
    let api = StoredValue::new(use_workspace().api.clone());
    let params = use_params_map();
    let navigate = StoredValue::new(use_navigate());
    let token = params.read_untracked().get("token").unwrap_or_default();

    let preview = RwSignal::new(None::<crate::api::workspace::InvitePreview>);
    let problem = RwSignal::new(None::<String>);
    let joined = RwSignal::new(None::<String>);
    let held = StoredValue::new(token.clone());

    spawn_local({
        let token = token.clone();

        async move {
            match api.get_value().preview_invite(&token).await {
                Ok(found) => preview.set(Some(found.invite)),
                Err(why) => problem.set(Some(why.to_string())),
            }
        }
    });

    let accept = move |_| {
        problem.set(None);

        spawn_local(async move {
            match api.get_value().accept_invite(&held.get_value()).await {
                Ok(answer) => {
                    joined.set(Some(answer.workspace.name));

                    // The workspace list on the account is stale now; a reload is the honest way
                    // to refresh it, and it happens once.
                    gloo_timers::future::TimeoutFuture::new(1200).await;

                    if let Some(window) = web_sys::window() {
                        let _ = window.location().assign("/library");
                    }
                }
                Err(why) => problem.set(Some(why.to_string())),
            }
        });
    };

    view! {
        <div class="mx-auto max-w-md p-6">
            <h2 class="mb-3 text-xl font-semibold" data-testid="screen-title">"Invitation"</h2>

            <Show when=move || problem.get().is_some()>
                <p class="mb-3 rounded border border-red-300 bg-red-50 p-2 text-sm text-red-800">
                    {move || problem.get()}
                </p>
            </Show>

            {move || match (joined.get(), preview.get()) {
                (Some(name), _) => view! {
                    <p class="text-sm">
                        {format!("You have joined {name}. Taking you to the library…")}
                    </p>
                }
                .into_any(),

                (None, None) => view! {
                    <p class="text-sm text-slate-500">"Checking the link…"</p>
                }
                .into_any(),

                (None, Some(invite)) => {
                    let spent = invite.used || invite.expired;

                    view! {
                        <div class="text-sm">
                            <p class="mb-3">
                                "You have been invited to "
                                <strong>{invite.workspace_name.clone()}</strong>
                                {format!(" as {}, at ", invite.role)}
                                <strong>{invite.email.clone()}</strong>
                                "."
                            </p>

                            {invite.used.then(|| view! {
                                <p class="mb-3 text-amber-700 dark:text-amber-400">
                                    "This invitation has already been used."
                                </p>
                            })}
                            {invite.expired.then(|| view! {
                                <p class="mb-3 text-amber-700 dark:text-amber-400">
                                    "This invitation has expired."
                                </p>
                            })}

                            <div class="flex gap-3">
                                <button
                                    class="rounded bg-slate-900 px-4 py-2 text-white disabled:opacity-40 dark:bg-slate-100 dark:text-slate-900"
                                    data-testid="accept-invite"
                                    prop:disabled=spent
                                    on:click=accept
                                >
                                    "Accept"
                                </button>
                                <button
                                    class="underline"
                                    on:click=move |_| {
                                        navigate.get_value()("/library", Default::default())
                                    }
                                >
                                    "Not now"
                                </button>
                            </div>
                        </div>
                    }
                    .into_any()
                }
            }}
        </div>
    }
}
