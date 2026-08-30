mod config;
mod db;
mod timer;

fn main() {
    let _ = (config::default_path(), timer::now_ms());
}
