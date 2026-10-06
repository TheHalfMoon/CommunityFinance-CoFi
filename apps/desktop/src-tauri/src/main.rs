fn main() {
    if let Err(error) = cofi_desktop_lib::run() {
        eprintln!("CoFi desktop failed: {error}");
        std::process::exit(1);
    }
}
