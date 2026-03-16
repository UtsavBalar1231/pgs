use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    match pgs::mcp::server::run_stdio().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(4)
        }
    }
}
