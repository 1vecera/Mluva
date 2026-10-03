fn main() -> std::process::ExitCode {
    // The released narration wrapper ignores arguments and accepts control on stdin.
    if let Err(code) = mluva_workflows::launch::inherit_managed_profile(std::env::args_os().take(1))
    {
        eprintln!("{}", mluva_workflows::launch::START_ERROR);
        return std::process::ExitCode::from(code);
    }
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => {
            eprintln!(
                "Annotation could not finish. Check Mluva's microphone and speech provider, then try again."
            );
            return std::process::ExitCode::FAILURE;
        }
    };
    std::process::ExitCode::from(runtime.block_on(mluva_workflows::narration::run_cli()))
}
