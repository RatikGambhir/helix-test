//! Dedicated server: `cubeterra-server [--port 25575] [--world worlds/world] [--seed N] [--view 12] [--no-mobs]`.

use cubeterra_server::{Config, start};
use std::path::PathBuf;

fn main() {
    let mut cfg = Config::default();
    let mut port = cubeterra_common::DEFAULT_PORT;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--port" => port = args.next().and_then(|v| v.parse().ok()).unwrap_or(port),
            "--world" => cfg.world_dir = PathBuf::from(args.next().unwrap_or_default()),
            "--seed" => cfg.seed = args.next().and_then(|v| v.parse().ok()).unwrap_or(cfg.seed),
            "--view" => cfg.max_view = args.next().and_then(|v| v.parse().ok()).unwrap_or(cfg.max_view),
            "--motd" => cfg.motd = args.next().unwrap_or_default(),
            "--no-mobs" => cfg.mobs = false,
            "-h" | "--help" => {
                println!("cubeterra-server [--port 25575] [--world worlds/world] [--seed N] [--view 12] [--motd TEXT] [--no-mobs]");
                return;
            }
            other => eprintln!("ignoring unknown argument {other}"),
        }
    }
    cfg.bind = format!("0.0.0.0:{port}");
    let server = match start(cfg.clone()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("could not start the server on port {port}: {e}");
            std::process::exit(1);
        }
    };
    println!("Cubeterra server listening on port {port} (world: {})", cfg.world_dir.display());
    println!("Press Enter to stop and save.");
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    if line.is_empty() {
        // No terminal attached (running as a service): keep serving.
        loop {
            std::thread::park();
        }
    }
    drop(server);
    println!("World saved. Bye!");
}
