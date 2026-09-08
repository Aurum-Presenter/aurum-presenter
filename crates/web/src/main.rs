//! The client's entry point. Everything it is made of lives in the library beside it.

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(aurum_web::routes::App);
}
