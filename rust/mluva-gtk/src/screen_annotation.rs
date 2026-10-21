//! Draw on the screen while dictating on Hyprland: hold Shift and move to ink, Shift-click to
//! pin. Marked recordings deliver keyframes and a short summary with the transcript.
pub mod hyprland;
mod overlay;
pub mod session;

pub use session::AnnotationSession;

/// `mluva --annotation-cleanup <frames directory> <token>`: the crash-cleanup watchdog.
pub fn run_cleanup(arguments: &[String]) -> Option<u8> {
    match arguments {
        [_, flag, directory, token] if flag == "--annotation-cleanup" => {
            Some(session::run_cleanup(std::path::Path::new(directory), token).clamp(0, 255) as u8)
        }
        [_, flag, ..] if flag == "--annotation-cleanup" => Some(2),
        _ => None,
    }
}
