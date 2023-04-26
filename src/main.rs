mod jobs_scheduler;
mod droplet_store;
mod config;
mod client;

// because it breaks debugger =(
#[cfg(not(debug_assertions))]
#[global_allocator]
static GLOBAL_MIMALLOC: mimalloc_rust::GlobalMiMalloc = mimalloc_rust::GlobalMiMalloc;

fn main() {
    println!("Hello, world!");
}
