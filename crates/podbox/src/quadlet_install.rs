//! Quadlet/systemd install and uninstall for a container definition.

pub mod paths;
pub mod preflight;
pub mod targets;
pub mod units;

mod install;
mod transaction;
mod uninstall;

pub use install::{install, install_with_ctx};
pub use paths::{is_installed, list_installed_names};
pub use uninstall::uninstall;

#[cfg(test)]
mod tests {
    use super::paths::{application_container_path, flat_container_path};

    #[test]
    fn flat_and_application_paths_differ() {
        let flat = flat_container_path("myenv");
        let app = application_container_path("myenv");
        assert!(flat.to_string_lossy().ends_with("myenv.container"));
        assert!(
            app.to_string_lossy().ends_with("myenv/myenv.container")
                || app.to_string_lossy().ends_with("myenv\\myenv.container")
        );
        assert_ne!(flat, app);
    }
}
