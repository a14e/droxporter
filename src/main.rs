mod do_json_protocol;
mod do_client;
mod config;
mod jobs_scheduler;
mod key_manager;
mod droplet_store;
mod rate_limiter;

// because it breaks debugger =(
#[cfg(not(debug_assertions))]
#[global_allocator]
static GLOBAL_MIMALLOC: mimalloc_rust::GlobalMiMalloc = mimalloc_rust::GlobalMiMalloc;

fn main() {
    println!("Hello, world!");
}
