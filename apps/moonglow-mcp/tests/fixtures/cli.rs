// Standalone fake CLI, compiled by the pipe test; intentionally no application dependencies.
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|s|s=="describe") {
        if let Ok(path)=std::env::var("FAKE_CLI_DESCRIPTION") {
            print!("{}",std::fs::read_to_string(path).unwrap());
        } else {std::process::exit(2);}
    } else {
        if args.iter().any(|s|s=="--delay") {std::thread::sleep(std::time::Duration::from_secs(5));}
        println!("{{\"invented\":true}}");
    }
}
