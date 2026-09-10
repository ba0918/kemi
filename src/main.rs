fn main() {
    if std::env::args().skip(1).any(|arg| arg == "--version") {
        println!("kemi {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    eprintln!("kemi: 入力モードを指定してください");
    std::process::exit(2);
}
