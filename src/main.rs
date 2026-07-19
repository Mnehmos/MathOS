use std::process::ExitCode;

use clap::Parser;
use mcl::cli::Cli;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let json = cli.json;
    match cli.execute() {
        Ok(outcome) => {
            let value = outcome.value;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&value).expect("JSON value")
                );
            } else if let Some(message) = value.get("message").and_then(|item| item.as_str()) {
                println!("{message}");
            } else {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&value).expect("JSON value")
                );
            }
            if outcome.success {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            if json {
                eprintln!(
                    "{}",
                    serde_json::to_string_pretty(&error).expect("serializable error")
                );
            } else {
                eprintln!("{}: {}", error.code, error.message);
                eprintln!("Suggested action: {}", error.corrective_action);
            }
            ExitCode::FAILURE
        }
    }
}
