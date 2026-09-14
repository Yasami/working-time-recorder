#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg_attr(not(windows), allow(dead_code))]
mod record;
#[cfg(windows)]
mod win;

fn main() {
    #[cfg(windows)]
    win::run();

    #[cfg(not(windows))]
    {
        eprintln!("working-time-viewer は Windows 専用です。");
        std::process::exit(1);
    }
}
