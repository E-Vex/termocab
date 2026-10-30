mod cli;
mod content;
mod engine;
mod progress;
mod session;
mod srs;
mod storage;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    cli::run(&args);
}
