use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Log level:
    ///     Off,
    ///     Error,
    ///     Warn,
    ///     Info,
    ///     Debug,
    ///     Trace
    #[arg(short, long)]
    pub log_level: Option<log::LevelFilter>,
}
