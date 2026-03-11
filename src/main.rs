use std::process::ExitCode;

fn main() -> ExitCode {
    let args = std::env::args().collect::<Vec<_>>();

    match cadenza::cli::parse_args(args) {
        cadenza::cli::ParsedArgs::ShowHelp(topic) => {
            print!("{}", cadenza::cli::help_text(topic));
            ExitCode::SUCCESS
        }
        cadenza::cli::ParsedArgs::Run(command) => ExitCode::from(cadenza::app::run(command) as u8),
    }
}
