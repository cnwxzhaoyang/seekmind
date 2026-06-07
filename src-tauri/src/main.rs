// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::env;

fn main() {
    let reset_storage = env::args().any(|arg| arg == "--reset-local-storage");
    if reset_storage {
        seekmind_lib::reset_local_storage().expect("failed to reset local SeekMind storage");
        env::set_var("SEEKMIND_SKIP_BOOTSTRAP_INDEX", "1");
    }

    seekmind_lib::run()
}
