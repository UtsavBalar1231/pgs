use std::process::ExitCode;

// One runtime thread makes handler tasks reach the per-repo mutation lock in
// the order rmcp spawned them, which is the order requests arrived, so a commit
// pipelined behind a stage commits what it staged. Git work runs on the blocking
// pool either way.
#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    match pgs::mcp::server::run_stdio().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(4)
        }
    }
}
