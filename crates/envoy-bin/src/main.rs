#![forbid(unsafe_code)]

// The config module's public items are consumed by `run()` starting in Task 8.
// Until then the binary target has no live use of them; `#[allow(dead_code)]`
// here suppresses the intermediate-task clippy noise without muting it crate-wide.
// Remove this attribute in Task 8 when main.rs calls `config::parse_bootstrap`.
#[allow(dead_code)]
mod config;

fn main() {
    // Replaced by Task 8 with the real wiring.
}
