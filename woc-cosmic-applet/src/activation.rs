use std::process::Command;

pub const MAIN_APP_ID: &str = "io.github.fernandox7.wocplayercount";
pub const MAIN_EXECUTABLE: &str = "woc-widget";

pub fn launch(exec: &str, activation_token: Option<&str>) {
    let mut command = Command::new(exec);
    if let Some(token) = activation_token {
        command.env("XDG_ACTIVATION_TOKEN", token);
    }
    if let Err(error) = command.spawn() {
        eprintln!("failed to launch {exec}: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_targets_the_installed_main_application() {
        assert_eq!(MAIN_APP_ID, "io.github.fernandox7.wocplayercount");
        assert_eq!(MAIN_EXECUTABLE, "woc-widget");
    }
}
