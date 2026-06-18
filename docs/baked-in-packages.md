---
title: Baked-in Base Packages
description: How podbox auto-installs a curated set of base packages at image build time, plus locale, timezone, and sudo provisioning.
---

# Baked-in Base Packages

For **custom builds** (non-prebuilt), `podbox build` auto-injects a curated set of base packages into the generated Containerfile based on the detected base image's distro family. You don't need to remember them in your TOML — they just work.

Prebuilt images (when `image.image` is set in the config) are not auto-injected — they're expected to ship complete. Override or extend the list under `[image.packages]`.

## What gets installed

| Group | Packages |
|---|---|
| **Privilege escalation** | `sudo` |
| **HTTP / retrieval** | `curl`, `wget` |
| **Archivers** | `tar`, `unzip` |
| **Locators** | `which` |
| **Coreutils family** | `coreutils`, `diffutils`, `findutils`, `grep`, `sed`, `gawk` |
| **Shell completion** | `bash-completion` (and `zsh-completions` on Arch) |
| **User shell** | `fish` / `bash` / `zsh` (whichever `$SHELL` points at on the host) |
| **Locales** | `locales` (Debian), `glibc-all-langpacks` (Fedora), `glibc` (Arch), `musl-locales` (Alpine) |

## Host shell detection

At build time, `podbox build` reads the host's `$SHELL` and adds the matching package plus any shell-completions package to the Containerfile. The guest entrypoint also detects the best available shell at runtime as a fallback.

| Host `$SHELL` | Injected packages |
|---|---|
| `/usr/bin/fish` | `fish` |
| `/bin/bash` | `bash`, `bash-completion` |
| `/usr/bin/zsh` | `zsh` (+ `zsh-completions` on Arch, `zsh-common` on Debian) |
| `/bin/sh`, `/bin/dash` | `dash` |

> For prebuilt images, the runtime shell comes from the image itself, and the build-time injection is skipped.

## Distro families

`podbox` detects the base image's distro family from its name and picks the correct package manager and install/clean commands:

| Family | Detected by | Manager | Install cmd | Clean cmd |
|---|---|---|---|---|
| Debian | `debian`, `ubuntu`, `mint`, `kali`, `pop`, `elementary` | `apt` | `apt-get update && apt-get install -y --no-install-recommends` | `rm -rf /var/lib/apt/lists/*` |
| Fedora | `fedora`, `rhel`, `centos`, `rocky`, `alma`, `nobara`, `cachy` | `dnf` | `dnf install -y` | `dnf clean all` |
| Arch | `arch`, `manjaro`, `endeavouros`, `garuda` | `pacman` | `pacman -Syu --noconfirm` | `pacman -Scc --noconfirm` |
| Alpine | `alpine` | `apk` | `apk add --no-cache` | _(none)_ |
| Unknown | everything else | `dnf` | `dnf install -y` | `dnf clean all` |

Unknown distros fall back to `dnf`. If your base image uses a different manager, set `image.packages.manager` explicitly to one of `apt`, `apt-get`, `apk`, `pacman`, `dnf` and `podbox` will use the matching install/remove commands.

## Overriding or extending

Add to `[image.packages].install` in your config — duplicates are removed:

```toml
[image.packages]
install = ["neovim", "git", "ripgrep"]
```

Anything in `install` is added on top of the auto-injected base list. If you want a lean image, leave `install` empty — you still get the base packages.

To *remove* packages shipped by the base image, use `[image.packages].remove`. The package-manager command is dispatched by family (see table above).

## Passwordless sudo

The guest entrypoint writes a NOPASSWD rule to `/etc/sudoers.d/podbox` when it creates the host user:

```
bet ALL=(ALL) NOPASSWD: ALL
```

`sudo apt install …`, `sudo dnf install …`, etc. all work without prompting. The rule is regenerated on every container start, so it's always in sync with `HOST_USER`.

## Locale & timezone sync

| What | Where | Behaviour |
|---|---|---|
| `LANG`, `LC_ALL`, `LC_CTYPE` | Quadlet `Environment=` (and Containerfile `ENV` for custom builds) | Pulled from the host's environment variables |
| `/etc/localtime` | Quadlet `Volume=/etc/localtime:/etc/localtime:ro` | Mounted only when the file exists on the host |
| `/etc/timezone` | Quadlet `Volume=/etc/timezone:/etc/timezone:ro` | Mounted only when the file exists on the host (Debian/Ubuntu convention) |
| Locales package | Containerfile `RUN` | Auto-installed (locales / glibc-all-langpacks / musl-locales) so the locale actually resolves inside the container |

> On some distros (notably Arch and Fedora), `/etc/timezone` does not exist — the file is replaced by a symlink at `/etc/localtime`. `podbox` checks both, so missing files just mean the corresponding mount is skipped.

## Modern XDG theme / icon / font paths

When `sync_themes`, `sync_icons`, or `sync_fonts` is enabled, `podbox` mounts both the legacy and modern XDG locations, *if they exist on the host*:

| Legacy | Modern |
|---|---|
| `~/.themes` | `~/.local/share/themes` |
| `~/.icons` | `~/.local/share/icons` |
| `~/.fonts` | `~/.local/share/fonts` |

Modern desktop environments (GNOME 45+, KDE Plasma 6) place user assets in `~/.local/share/…`, so without these mounts, themed apps look unstyled inside the container.
