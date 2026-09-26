#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg_attr(not(windows), allow(dead_code))]
mod config;
#[cfg_attr(not(windows), allow(dead_code))]
mod record;
// TODO: win::updater から使うようになったら、ほかと同じ cfg_attr(not(windows), allow(dead_code)) にする
#[allow(dead_code)]
mod update;
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
