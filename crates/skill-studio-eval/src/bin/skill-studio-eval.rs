use clap::Parser;

#[derive(Parser)]
#[command(about = "Test skills through installed agent clients")]
struct Cli {
    #[command(flatten)]
    args: skill_studio_eval::cli::EvalArgs,
}

fn main() -> std::process::ExitCode {
    skill_studio_eval::cli::entry(Cli::parse().args)
}
