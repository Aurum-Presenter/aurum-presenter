//! The app's two states, and everything reachable in the second one.
//!
//! Signed out, the only thing that exists is the auth screen. Signed in, every route reads from
//! the device and the network is a background concern.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::NavigateOptions;
use leptos_router::components::{ParentRoute, Route, Router, Routes};
use leptos_router::hooks::use_navigate;
use leptos_router::path;

use crate::api::{Api, RestoreResult};
use crate::app::shell::Shell;
use crate::app::{provide_workspace, storage};
use crate::auth::AuthScreen;
use crate::auth::local;
use crate::library::page::LibraryPage;
use crate::library::trash::TrashPage;
use crate::present::{AudiencePage, ControlPage, JoinPage, StagePage};
use crate::pwa::share::SharePage;
use crate::sets::{PrintPage, ReaderPage, SetPage, SetsPage};
use crate::settings::{
    AboutPage, AccountPage, ConflictsPage, InvitePage, MembersPage, StoragePage,
};
use crate::sheets::SheetViewerPage;
use crate::song::SongPage;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Loading,
    SignedOut,
    Ready,
}

#[component]
pub fn App() -> impl IntoView {
    let api = Api::default();
    let phase = RwSignal::new(Phase::Loading);
    let me = RwSignal::new(None::<crate::api::Account>);
    let local_mode = RwSignal::new(false);

    let load = {
        let api = api.clone();

        move || {
            let api = api.clone();

            spawn_local(async move {
                // A workspace started before signing in is handed to the server under the id it
                // already has, so nothing made offline has to be re-created.
                let _ = local::claim(&api).await;

                match api.me().await {
                    Ok(account) => {
                        me.set(Some(account));
                        local_mode.set(false);
                        phase.set(Phase::Ready);
                    }
                    Err(_) => phase.set(Phase::SignedOut),
                }
            });
        }
    };

    // On start: a device already working without an account keeps working. Otherwise the
    // refresh cookie decides, and a network failure is not a sign-out.
    {
        let api = api.clone();
        let load = load.clone();

        spawn_local(async move {
            if let Some(started) = local::local_mode() {
                // A sign-in screen would be a wall in front of songs that are right here.
                me.set(Some(local::local_account(&started)));
                local_mode.set(true);
                phase.set(Phase::Ready);

                return;
            }

            match api.restore().await {
                RestoreResult::Ok => load(),
                RestoreResult::SignedOut => phase.set(Phase::SignedOut),
                // Offline with no account on this device: there is nothing to show and nothing
                // to sign in against, so the screen that asks is still the right one.
                RestoreResult::Offline => phase.set(Phase::SignedOut),
            }
        });
    }

    let sign_out = {
        let api = api.clone();

        Callback::new(move |()| {
            let api = api.clone();

            spawn_local(async move {
                let _ = api.sign_out().await;

                local::end();
                storage::remove(storage::WORKSPACE);
                me.set(None);
                phase.set(Phase::SignedOut);
            });
        })
    };

    let signed_in = {
        let load = load.clone();

        Callback::new(move |()| load())
    };

    let started_local = Callback::new(move |mode: local::LocalMode| {
        me.set(Some(local::local_account(&mode)));
        local_mode.set(true);
        phase.set(Phase::Ready);
    });

    let auth_api = api.clone();

    view! {
        {move || match phase.get() {
            Phase::Loading => view! {
                <div class="flex min-h-dvh items-center justify-center text-slate-500">
                    "Opening your library…"
                </div>
            }
            .into_any(),

            Phase::SignedOut => view! {
                <AuthScreen
                    api=auth_api.clone()
                    on_signed_in=signed_in
                    on_local_mode=started_local
                />
            }
            .into_any(),

            Phase::Ready => {
                let account = me.get().expect("an account once ready");

                view! {
                    <SignedIn
                        me=account
                        local=local_mode.get()
                        api=api.clone()
                        on_sign_out=sign_out
                    />
                }
                .into_any()
            }
        }}
    }
}

#[component]
fn SignedIn(
    me: crate::api::Account,
    local: bool,
    api: Api,
    on_sign_out: Callback<()>,
) -> impl IntoView {
    provide_workspace(me, local, api);

    view! {
        <Router>
            <Routes fallback=|| view! { <Redirect to="/library" /> }>
                // The output windows sit outside the shell: they are screens in a room, not
                // pages in an app, and a projector must never show a navigation bar.
                <Route path=path!("/output/audience") view=AudiencePage />
                <Route path=path!("/output/stage") view=StagePage />

                <ParentRoute path=path!("/") view=move || view! { <Shell on_sign_out /> }>
                    <Route path=path!("") view=LibraryPage />
                    <Route path=path!("library") view=LibraryPage />
                    <Route path=path!("library/folder/:folder_id") view=LibraryPage />
                    <Route path=path!("library/trash") view=TrashPage />
                    <Route path=path!("song/:song_id") view=|| view! { <SongPage /> } />
                    <Route
                        path=path!("song/:song_id/edit")
                        view=|| view! { <SongPage edit=true /> }
                    />
                    <Route path=path!("song/:song_id/sheet/:sheet_id") view=SheetViewerPage />
                    <Route path=path!("sets") view=SetsPage />
                    <Route path=path!("sets/:set_id") view=SetPage />
                    <Route path=path!("sets/:set_id/read/:index") view=ReaderPage />
                    <Route path=path!("sets/:set_id/print") view=PrintPage />
                    <Route path=path!("join") view=JoinPage />
                    <Route path=path!("present/:session_id") view=ControlPage />
                    <Route path=path!("invite/:token") view=InvitePage />
                    <Route path=path!("share") view=SharePage />
                    <Route path=path!("settings/account") view=AccountPage />
                    <Route path=path!("settings/members") view=MembersPage />
                    <Route path=path!("settings/sync/conflicts") view=ConflictsPage />
                    <Route path=path!("settings/storage") view=StoragePage />
                    <Route path=path!("settings/trash") view=TrashPage />
                    <Route path=path!("settings/about") view=AboutPage />
                </ParentRoute>
            </Routes>
        </Router>
    }
}

/// An unknown path lands in the library rather than on an error: every route in this app is
/// client-side, and a stale bookmark is not a failure worth a screen of its own.
#[component]
fn Redirect(to: &'static str) -> impl IntoView {
    let navigate = use_navigate();

    Effect::new(move |_| {
        navigate(
            to,
            NavigateOptions {
                replace: true,
                ..NavigateOptions::default()
            },
        )
    });

    view! { <span /> }
}
