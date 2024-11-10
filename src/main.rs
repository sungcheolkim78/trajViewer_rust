// Copyright 2023 SungcheolKim. All rights reserved.

use std::process;
use clap::Parser;
use traj_viewer::Config;

#[tokio::main]
async fn main() {
    let config = Config::parse();
    println!("{:?}", config);

    if let Err(e) = traj_viewer::run(config) {
        eprintln!("Application error: {e}");
        process::exit(1);
    }
}
