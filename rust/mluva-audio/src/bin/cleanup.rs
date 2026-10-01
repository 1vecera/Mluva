use std::path::Path;

fn main() {
    let mut arguments = std::env::args_os().skip(1);
    let Some(directory) = arguments.next() else {
        std::process::exit(2);
    };
    if arguments.next().is_some() {
        std::process::exit(2);
    }
    if mluva_audio::volatile::run_cleanup(Path::new(&directory)).is_err() {
        std::process::exit(1);
    }
}
