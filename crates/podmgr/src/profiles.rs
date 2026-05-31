/// A named, ready-to-use configuration template.
pub struct Profile {
    pub name: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub toml: &'static str,
}

/// List all available profiles.
pub fn all() -> Vec<Profile> {
    vec![cachy(), fedora(), gaming()]
}

/// Find a profile by name (case-insensitive).
pub fn find(name: &str) -> Option<Profile> {
    let lower = name.to_lowercase();
    all().into_iter().find(|p| p.name == lower)
}

fn cachy() -> Profile {
    Profile {
        name: "cachy",
        label: "CachyOS",
        description: "CachyOS-based environment optimized for gaming",
        toml: include_str!("profiles/cachy.toml"),
    }
}

fn fedora() -> Profile {
    Profile {
        name: "fedora",
        label: "Fedora",
        description: "Fedora-based general-purpose environment",
        toml: include_str!("profiles/fedora.toml"),
    }
}

fn gaming() -> Profile {
    Profile {
        name: "gaming",
        label: "Gaming",
        description: "Generic gaming environment (distro-agnostic)",
        toml: include_str!("profiles/gaming.toml"),
    }
}

/// List profile names for tab completion / CLI hints.
pub fn list_names() -> Vec<&'static str> {
    all().into_iter().map(|p| p.name).collect()
}
