//! The client's entry point. Everything it is made of lives in the library beside it.

fn main() {
    console_error_panic_hook::set_once();

    // Before the app, so a device that has already been here once starts from the precached
    // shell rather than from the network.
    aurum_web::pwa::register_service_worker();
    aurum_web::pwa::install::count_visit();

    // The launch queue has exactly one consumer, and a later one gets nothing: a file opened
    // with Aurum has to be claimed before the app is mounted.
    aurum_web::pwa::share::accept_launch_files();

    leptos::mount::mount_to_body(aurum_web::routes::App);
}
