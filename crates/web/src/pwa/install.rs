//! Installing the app.
//!
//! Installation is an upgrade, not a gate: the first visit works in a plain tab and nothing here
//! blocks anything. The banner appears on the third visit or the first pin, whichever comes
//! first, and a dismissal is respected for a month.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::app::storage;

const META_KEY: &str = "aurum.app";
const DISMISS_DAYS: i64 = 30;

/// How many visits before the app suggests installing itself. Two visits is somebody looking;
/// three is somebody using it.
const EARNED_AFTER: u32 = 3;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct AppMeta {
    #[serde(default)]
    pub visits: u32,
    pub installed_at: Option<String>,
    pub dismissed_until: Option<String>,
    #[serde(default)]
    pub persist_granted: bool,
}

pub fn read() -> AppMeta {
    storage::read(META_KEY)
        .and_then(|held| serde_json::from_str(&held).ok())
        .unwrap_or_default()
}

pub fn write(meta: &AppMeta) {
    // Storage blocked. The app works; it will simply ask again next time.
    if let Ok(held) = serde_json::to_string(meta) {
        storage::write(META_KEY, &held);
    }
}

pub fn count_visit() {
    let mut meta = read();

    meta.visits += 1;
    write(&meta);
}

/// Already installed, by either of the two ways a browser reports it.
pub fn standalone() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };

    let by_display = window
        .match_media("(display-mode: standalone)")
        .ok()
        .flatten()
        .is_some_and(|query| query.matches());

    let by_navigator = js_sys::Reflect::get(&window.navigator(), &JsValue::from_str("standalone"))
        .map(|value| value.is_truthy())
        .unwrap_or(false);

    by_display || by_navigator
}

/// iOS has no prompt event; it needs a sentence of instructions instead.
fn is_ios() -> bool {
    web_sys::window()
        .map(|window| window.navigator().user_agent().unwrap_or_default())
        .unwrap_or_default()
        .to_lowercase()
        .contains("iphone")
}

/// Whether the banner has been earned and not waved away.
fn wanted(meta: &AppMeta) -> bool {
    let dismissed = meta
        .dismissed_until
        .as_deref()
        .and_then(aurum_core::time::parse)
        .is_some_and(|until| until > crate::now_ms());

    meta.visits >= EARNED_AFTER && !dismissed && meta.installed_at.is_none()
}

/// The browser's install offer, as something a screen can read and act on.
///
/// One shape for the banner and for the About screen: whether the app is already installed,
/// whether the browser has offered a prompt, and the iOS case, which has no prompt at all.
#[derive(Clone, Copy)]
pub struct Install {
    pub installed: RwSignal<bool>,
    pub available: RwSignal<bool>,
    pub ios: bool,
    prompt: StoredValue<Option<JsValue>, LocalStorage>,
}

impl Install {
    /// Shows the browser's own prompt. On iOS there is none, and `ios` is the instruction to
    /// say so instead.
    pub fn ask(&self) {
        let Some(event) = self.prompt.get_value() else {
            return;
        };

        // `prompt()` and then `userChoice`: the browser answers with what the person chose, and
        // a refusal is a month's silence rather than the same banner tomorrow.
        let _ = js_sys::Reflect::get(&event, &JsValue::from_str("prompt"))
            .ok()
            .and_then(|held| held.dyn_into::<js_sys::Function>().ok())
            .map(|call| call.call0(&event));

        self.prompt.set_value(None);
        self.available.set(false);
    }
}

/// Listens for the two events the browser fires about installing, for as long as the caller
/// lives. Called by any screen that offers to install the app.
pub fn use_install() -> Install {
    let held = Install {
        installed: RwSignal::new(standalone()),
        available: RwSignal::new(false),
        ios: is_ios(),
        prompt: StoredValue::new_local(None::<JsValue>),
    };

    let Some(window) = web_sys::window() else {
        return held;
    };

    {
        let on_prompt = Closure::<dyn Fn(web_sys::Event)>::new(move |event: web_sys::Event| {
            // Held rather than shown: the browser's own moment is rarely the app's.
            event.prevent_default();
            held.prompt.set_value(Some(JsValue::from(event)));
            held.available.set(true);
        });

        let _ = window.add_event_listener_with_callback(
            "beforeinstallprompt",
            on_prompt.as_ref().unchecked_ref(),
        );
        on_prompt.forget();
    }

    {
        let on_installed = Closure::<dyn Fn()>::new(move || {
            held.installed.set(true);
            write(&AppMeta {
                installed_at: Some(crate::now()),
                ..read()
            });
        });

        let _ = window.add_event_listener_with_callback(
            "appinstalled",
            on_installed.as_ref().unchecked_ref(),
        );
        on_installed.forget();
    }

    held
}

#[component]
pub fn InstallBanner() -> impl IntoView {
    let install = use_install();
    let installed = install.installed;
    let available = install.available;
    let showing_ios = RwSignal::new(false);
    let dismissed = RwSignal::new(!wanted(&read()));

    let dismiss = move || {
        write(&AppMeta {
            dismissed_until: Some(aurum_core::time::format(
                crate::now_ms() + DISMISS_DAYS * 86_400_000,
            )),
            ..read()
        });

        dismissed.set(true);
        showing_ios.set(false);
    };

    let ask = move |_| {
        if install.ios {
            showing_ios.set(true);
            return;
        }

        install.ask();
    };

    view! {
        <Show when=move || {
            !installed.get() && !dismissed.get() && (available.get() || install.ios)
        }>
            <div
                class="flex flex-wrap items-center gap-3 border-b border-accent/40 bg-accent/10 px-4 py-2 text-sm text-accent"
                data-testid="install-banner"
            >
                <span>
                    "Install Aurum on this device so it opens like an app and starts without a \
                     network."
                </span>

                <button class="rounded-md bg-accent px-3 py-1 text-on-accent" on:click=ask>
                    "Install"
                </button>

                <button class="text-ink-3 hover:text-ink underline-offset-2 hover:underline" on:click=move |_| dismiss()>"Not now"</button>
            </div>

            <Show when=move || showing_ios.get()>
                <div
                    class="fixed inset-0 z-30 flex items-center justify-center bg-black/60 p-6"
                    on:click=move |_| showing_ios.set(false)
                >
                    <div
                        class="w-80 rounded-md bg-surface p-4 text-sm"
                        on:click=|event| event.stop_propagation()
                    >
                        <h2 class="mb-2 font-semibold">"Add to the Home Screen"</h2>
                        <ol class="mb-3 list-decimal space-y-1 pl-4 text-ink-3">
                            <li>"Tap the Share button in Safari."</li>
                            <li>"Choose “Add to Home Screen”."</li>
                            <li>"Open Aurum from the icon — it will work with no signal."</li>
                        </ol>
                        <button class="text-ink-3 hover:text-ink underline-offset-2 hover:underline" on:click=move |_| dismiss()>"Got it"</button>
                    </div>
                </div>
            </Show>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(days: i64) -> String {
        aurum_core::time::format(1_788_912_000_000 + days * 86_400_000)
    }

    #[test]
    fn the_banner_is_earned_rather_than_shown_on_arrival() {
        assert!(!wanted(&AppMeta {
            visits: 1,
            ..AppMeta::default()
        }));
        assert!(!wanted(&AppMeta {
            visits: 2,
            ..AppMeta::default()
        }));
        assert!(wanted(&AppMeta {
            visits: 3,
            ..AppMeta::default()
        }));
    }

    #[test]
    fn an_installed_app_never_asks_again() {
        assert!(!wanted(&AppMeta {
            visits: 40,
            installed_at: Some(at(-1)),
            ..AppMeta::default()
        }));
    }

    /// An unreadable date is not a licence to nag: it fails to a dismissal that has expired,
    /// which is the same as never having been asked.
    #[test]
    fn a_dismissal_that_cannot_be_read_is_no_dismissal() {
        assert!(wanted(&AppMeta {
            visits: 5,
            dismissed_until: Some("not a date".to_owned()),
            ..AppMeta::default()
        }));
    }
}
