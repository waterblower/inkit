fn main() {
    let result =
        ink_lsp::protocol::run(&mut std::io::stdin().lock(), &mut std::io::stdout().lock());
    match result {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("Ink LSP: {error}");
            std::process::exit(1);
        }
    }
}
