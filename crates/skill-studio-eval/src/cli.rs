use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use clap::{Args, Subcommand};

use crate::engine::Lab;
use crate::store::{read_json, RunState};
use crate::suite::{EvaluationOutcome, TestSuite};

#[derive(Args)]
pub struct EvalArgs {
    /// Folder for saved suites and run evidence.
    #[arg(long, global = true)]
    pub data_dir: Option<PathBuf>,
    #[command(subcommand)]
    pub command: EvalCommand,
}

#[derive(Subcommand)]
pub enum EvalCommand {
    /// Inspect installed clients. This does not launch a model request.
    Clients,
    /// Validate a suite without running its clients or commands.
    Validate { suite: PathBuf },
    /// Freeze the suite inputs, run each trial, and print its saved result.
    Run { suite: PathBuf },
    /// List saved runs, newest first.
    History,
    /// Print a saved run as JSON, including check evidence.
    Show { id: String },
    /// Export the suite from a saved run; results remain separate.
    ExportSuite { id: String },
}

pub fn default_root() -> Result<PathBuf, String> {
    dirs::data_dir()
        .map(|root| root.join("com.skillstudio.app").join("skill-lab"))
        .ok_or_else(|| "Could not find the application data folder; set --data-dir".into())
}

#[allow(clippy::print_stdout, clippy::print_stderr)]
pub fn entry(args: EvalArgs) -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    match runtime.block_on(execute(args)) {
        Ok((output, success)) => {
            println!("{output}");
            if success {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

async fn execute(args: EvalArgs) -> Result<(String, bool), String> {
    if matches!(args.command, EvalCommand::Clients) {
        return Ok((
            serde_json::to_string_pretty(&crate::clients::inspect_clients().await)
                .map_err(|e| e.to_string())?,
            true,
        ));
    }
    if let EvalCommand::Validate { suite } = &args.command {
        let suite: TestSuite = read_json(suite)?;
        suite.validate()?;
        return Ok((
            format!(
                "Valid suite: {} ({} planned client runs)",
                suite.name,
                suite.trial_count()
            ),
            true,
        ));
    }
    let lab = Arc::new(Lab::open(args.data_dir.map_or_else(default_root, Ok)?)?);
    let output = match args.command {
        EvalCommand::Run { suite: path } => {
            let mut suite: TestSuite = read_json(&path)?;
            let parent = path
                .canonicalize()
                .map_err(|e| e.to_string())?
                .parent()
                .ok_or("Suite path has no parent")?
                .to_path_buf();
            if !std::path::Path::new(&suite.skill_path).is_absolute() {
                suite.skill_path = parent.join(&suite.skill_path).to_string_lossy().into();
            }
            for variant in &mut suite.variants {
                if let crate::suite::SkillVariant::Baseline { path } = &mut variant.skill {
                    if !std::path::Path::new(path).is_absolute() {
                        *path = parent.join(&*path).to_string_lossy().into();
                    }
                }
            }
            let prepared = lab.prepare(suite)?;
            let id = prepared.id.clone();
            let running = lab.execute(prepared);
            tokio::pin!(running);
            let result = tokio::select! {
                result = &mut running => result?,
                _ = tokio::signal::ctrl_c() => { lab.cancel(&id)?; running.await? }
            };
            let success = result.state == RunState::Completed
                && result
                    .trials
                    .iter()
                    .all(|t| t.outcome == EvaluationOutcome::Passed);
            return Ok((
                serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?,
                success,
            ));
        }
        EvalCommand::History => {
            serde_json::to_string_pretty(&lab.store.summaries()?).map_err(|e| e.to_string())?
        }
        EvalCommand::Show { id } => lab.export(&id)?,
        EvalCommand::ExportSuite { id } => {
            serde_json::to_string_pretty(&lab.rerun_suite(&id)?).map_err(|e| e.to_string())?
        }
        EvalCommand::Clients | EvalCommand::Validate { .. } => {
            return Err("Command was already handled".into())
        }
    };
    Ok((output, true))
}
