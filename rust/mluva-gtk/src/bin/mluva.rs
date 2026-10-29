fn main() -> glib::ExitCode {
    let arguments: Vec<_> = std::env::args().collect();
    if let Some(code) = mluva_gtk::screen_annotation::run_cleanup(&arguments) {
        return glib::ExitCode::from(code);
    }
    if let Some(code) = mluva_gtk::herdr_cli::run(&arguments) {
        return glib::ExitCode::from(code);
    }
    if let Err(code) = mluva_workflows::launch::inherit_managed_profile(std::env::args_os()) {
        eprintln!("{}", mluva_workflows::launch::START_ERROR);
        return glib::ExitCode::from(code);
    }
    mluva_gtk::bootstrap::run(&arguments)
}
