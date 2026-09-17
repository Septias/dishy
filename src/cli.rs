use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub(crate) struct Cli {
    /// File of the dish calulator.
    #[arg(short, long, default_value = "./plan.md")]
    pub plan: PathBuf,

    /// Root path under which all dishes can be found.
    #[arg(short, long, default_value = "./cookbook")]
    pub dish_root: PathBuf,
}
