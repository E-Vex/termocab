mod cli;
mod content;
mod engine;
mod lang;
mod progress;
mod session;
mod srs;
mod storage;

#[cfg(test)]
mod testenv;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(e) = cli::run(&args) {
        eprintln!("termocab: {}", e.message());
        std::process::exit(e.exit_code());
    }
}
