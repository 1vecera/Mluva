fn main() -> glib::ExitCode {
    let arguments: Vec<_> = std::env::args().collect();
    if let Some(code) = mluva_gtk::herdr_cli::run(&arguments) {
        return glib::ExitCode::from(code);
    }
    if let Err(code) = mluva_workflows::launch::inherit_managed_profile(std::env::args_os()) {
        eprintln!("{}", mluva_workflows::launch::START_ERROR);
        return glib::ExitCode::from(code);
    }
    if arguments.len() == 2 && arguments[1] == "--narrate" {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(_) => {
                eprintln!(
                    "Annotation could not finish. Check Mluva's microphone and speech provider, then try again."
                );
                return glib::ExitCode::FAILURE;
            }
        };
        return glib::ExitCode::from(runtime.block_on(mluva_workflows::narration::run_cli()));
    }
    mluva_gtk::bootstrap::run(&arguments)
}
