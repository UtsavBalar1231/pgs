use std::process::ExitCode;

use agstage::cmd;

fn main() -> ExitCode {
    match cmd::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let code = e.exit_code();
            let err_json = serde_json::json!({
                "error": e.to_string(),
                "exit_code": code,
            });
            println!("{err_json}");
            ExitCode::from(u8::try_from(code).expect("exit code fits in u8"))
        }
    }
}
