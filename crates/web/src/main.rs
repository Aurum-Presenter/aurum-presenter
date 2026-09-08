//! The client.
//!
//! Client-rendered, deliberately: the app has to start from a precached shell with the radio off,
//! which rules out server-side rendering and hydration.
use leptos::prelude::*;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}

#[component]
fn App() -> impl IntoView {
    view! {
        <main class="p-6">
            <h1 class="text-xl font-semibold">"Aurum Presenter"</h1>
            <p>"Rules from aurum-core " {aurum_core::version()}</p>
        </main>
    }
}
