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
        // A broken pipe exits the way cat and git do: silently, 141
        // (128 + SIGPIPE) — the reader of the pipe had enough (D6).
        if !e.quiet() {
            eprintln!("termocab: {}", e.message());
        }
        std::process::exit(e.exit_code());
    }
}
