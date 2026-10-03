#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    std::process::ExitCode::from(mluva_workflows::narration::run_cli().await)
}
