---
title: Base packages
description: Packages podbox installs into custom-built images automatically — base tools, shells, locales, sudo, and timezone.
---

# Base packages

Custom builds get a curated toolset injected automatically. You don't list
these in your TOML — they just show up.

Prebuilt images (`image.image` set) ship complete and skip this entirely.
Extend either kind under `[image.packages]`.

## What's included

| Group | Packages |
|---|---|
| Privilege escalation | `sudo` |
| Downloading | `curl`, `wget` |
| Archives | `tar`, `unzip` |
| Locators | `which` |
| Core tools | `coreutils`, `diffutils`, `findutils`, `grep`, `sed`, `gawk` |
| Shell completion | `bash-completion` (plus `zsh-completions` on Arch) |
| Your shell | `fish`, `bash`, or `zsh` — whichever `$SHELL` points at on the host |
| Locales | `locales` (Debian), `glibc-all-langpacks` (Fedora), `glibc` (Arch), `musl-locales` (Alpine) |

| Host `$SHELL` | Injected |
|---|---|
| `/usr/bin/fish` | `fish` |
| `/bin/bash` | `bash`, `bash-completion` |
| `/usr/bin/zsh` | `zsh` (+ `zsh-completions` on Arch, `zsh-common` on Debian) |
| `/bin/sh`, `/bin/dash` | `dash` |

Prebuilt images skip this — their shell comes from the image itself.

## Distro detection

The base image name picks the package manager:

| Family | Matched by | Manager |
|---|---|---|
| Debian | `debian`, `ubuntu`, `mint`, `kali`, `pop`, `elementary` | `apt` |
| Fedora | `fedora`, `rhel`, `centos`, `rocky`, `alma`, `nobara` | `dnf` |
| Arch | `arch`, `cachy`, `manjaro`, `endeavouros`, `garuda` | `pacman` |
| Alpine | `alpine` | `apk` |
| Anything else | — | `dnf` |

Wrong guess? Pin it:

```toml
[image.packages]
manager = "apk"   # dnf, apt, pacman, apk, zypper
```

Anything else is ignored and detection applies.

## Add or remove packages

```toml
[image.packages]
install = ["neovim", "git", "ripgrep"]   # added on top, duplicates removed
remove = ["vim-minimal"]                 # strip what the base image ships
```

Leave `install` empty for a lean image — you still get the base set.

## Passwordless sudo

`sudo dnf install …` just works, no prompt. The rule regenerates on every
start:

```
bet ALL=(ALL) NOPASSWD: ALL
```

## Locale and timezone

| What | How |
|---|---|
| `LANG`, `LC_ALL`, `LC_CTYPE` | Copied from your host environment |
| `/etc/localtime` | Mounted read-only, if it exists on the host |
| `/etc/timezone` | Mounted read-only, if it exists (Debian/Ubuntu) |

Missing files are skipped, not errors. Arch and Fedora don't have
`/etc/timezone` — that's normal.

## Theme, icon, and font paths

With `sync_themes`, `sync_icons`, or `sync_fonts` on, podbox mounts both the
old and new locations (when they exist on the host), so themed apps don't
look broken:

| Old | New |
|---|---|
| `~/.themes` | `~/.local/share/themes` |
| `~/.icons` | `~/.local/share/icons` |
| `~/.fonts` | `~/.local/share/fonts` |
