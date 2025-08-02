use zellix::*;

fn main() {
    rune::cli::Entry::new()
        .about(format!("Zellix's entrypoint for the"))
        .context(&mut |_opts| Ok(ConfigManager::context().unwrap()))
        .run()
}
