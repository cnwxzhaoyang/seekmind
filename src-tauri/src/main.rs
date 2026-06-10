// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::env;

fn main() {
    let _ = env::var("SEEKMIND_FORCE_FIRST_LAUNCH");
    seekmind_lib::run()
}
