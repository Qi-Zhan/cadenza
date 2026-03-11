use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandKind {
    RunUi { library_root: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedArgs {
    Run(CommandKind),
    ShowHelp(HelpTopic),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpTopic {
    Root,
}

pub fn parse_args<I>(args: I) -> ParsedArgs
where
    I: IntoIterator<Item = String>,
{
    let args = args.into_iter().collect::<Vec<_>>();

    match args.as_slice() {
        [_bin] => ParsedArgs::Run(CommandKind::RunUi {
            library_root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        }),
        [_bin, flag] if flag == "-h" || flag == "--help" => ParsedArgs::ShowHelp(HelpTopic::Root),
        [_bin, path] => ParsedArgs::Run(CommandKind::RunUi {
            library_root: path.into(),
        }),
        _ => ParsedArgs::ShowHelp(HelpTopic::Root),
    }
}

pub fn help_text(topic: HelpTopic) -> &'static str {
    match topic {
        HelpTopic::Root => {
            "cadenza\n\nUsage:\n  cadenza [path]\n\nArguments:\n  [path]            Scan and play music from this directory (defaults to current directory)\n\nOptions:\n  -h, --help        Show this help text\n"
        }
    }
}
