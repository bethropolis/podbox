use crate::config::Config;

/// Generate a Containerfile string from the config.
///
/// When `config.image.prebuilt` is true, emits a minimal Containerfile
/// (FROM + ENV + ENTRYPOINT + CMD only) since the base image already
/// contains everything needed.
pub fn generate(config: &Config, _guest_binary_name: &str) -> String {
    if config.image.prebuilt {
        return generate_prebuilt(config);
    }
    generate_custom(config)
}

fn generate_prebuilt(config: &Config) -> String {
    let mut lines: Vec<String> = Vec::new();

    lines.push(format!("FROM {}", config.image.base));
    lines.push(String::new());

    lines.push(format!(
        "ENV PODMGR_CONTAINER={}",
        config.container.name
    ));
    lines.push(String::new());

    lines.push("ENTRYPOINT [\"/usr/local/bin/podmgr-entry\"]".into());
    lines.push(format!(
        "CMD [\"{}\"]",
        config.container.shell
    ));

    lines.join("\n")
}

fn generate_custom(config: &Config) -> String {
    let mut lines: Vec<String> = Vec::new();

    lines.push(format!("FROM {}", config.image.base));
    lines.push(String::new());

    // Packages
    if !config.image.packages.install.is_empty() {
        let pkgs = config.image.packages.install.join(" ");
        lines.push(format!(
            "RUN dnf install -y {} && dnf clean all",
            pkgs
        ));
        lines.push(String::new());
    }

    if !config.image.packages.remove.is_empty() {
        let pkgs = config.image.packages.remove.join(" ");
        lines.push(format!("RUN dnf remove -y {} && dnf clean all", pkgs));
        lines.push(String::new());
    }

    // Custom RUN steps
    for cmd in &config.image.run.commands {
        lines.push(format!("RUN {}", cmd));
    }
    if !config.image.run.commands.is_empty() {
        lines.push(String::new());
    }

    // Integration layer
    lines.push("COPY podmgr-guest /usr/local/bin/podmgr-guest".into());
    lines.push("COPY podmgr-entry.sh /usr/local/bin/podmgr-entry".into());
    lines.push(
        "RUN chmod +x /usr/local/bin/podmgr-guest /usr/local/bin/podmgr-entry".into(),
    );
    lines.push(String::new());

    // Container name env
    lines.push(format!(
        "ENV PODMGR_CONTAINER={}",
        config.container.name
    ));
    lines.push(String::new());

    // ENTRYPOINT and CMD
    lines.push("ENTRYPOINT [\"/usr/local/bin/podmgr-entry\"]".into());
    lines.push(format!(
        "CMD [\"{}\"]",
        config.container.shell
    ));

    lines.join("\n")
}

/// Generate the entry script that is copied into the build context.
pub fn generate_entry_script() -> String {
    r#"#!/bin/sh
exec /usr/local/bin/podmgr-guest --entry "$@"
"#
    .into()
}
