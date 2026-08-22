fn main() {
    if let Err(error) = pod0_cli::run(std::env::args().skip(1)) {
        eprintln!("pod0-cli: {error}");
        std::process::exit(1);
    }
}
