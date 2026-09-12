fn main() {
    std::process::exit(rom_manager_lib::run_cli(std::env::args().skip(1).collect()))
}
