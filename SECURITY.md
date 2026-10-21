# Security policy

## Reporting a vulnerability

Please **don't open a public issue** for a security problem.

Report it privately through GitHub:
[Security → Report a vulnerability](https://github.com/bethropolis/podbox/security/advisories/new).

Include what you can of:
- the podbox version (`podbox --version`) and podman version;
- the config (`podbox.toml`) or the minimal part of it needed;
- steps to reproduce, and what an attacker gains.

You should get an acknowledgement within a week. Once a fix is ready we
publish an advisory and credit you, unless you'd rather stay anonymous.

## Supported versions

podbox is pre-1.0. Fixes go into the latest release only, so upgrade
before reporting if you can.

| Version | Supported |
|---------|-----------|
| latest 0.x release | yes |
| older releases | no |

## What podbox protects, and what it doesn't

podbox keeps development environments **separate** from your host. It is not
a hardened sandbox for running hostile code. Read this before relying on it.

A podbox container is a rootless podman container. Inside it you run as your
own user ID (`UserNS=keep-id`), with your own permissions on anything shared.
By default:
- the container gets its own home directory, not your `$HOME`;
- `NoNewPrivileges` is on;
- SELinux process labelling is **off** (`security_label_disable = true`),
  because Wayland and other shared sockets need it;
- integrations are opt-in.

Each integration you turn on widens what the container can reach:

| You enable | The container can |
|------------|-------------------|
| `wayland`, `audio`, `gpu` | use your compositor, sound server and GPU directly |
| `dbus` | call the D-Bus services allowed by `[dbus]` through `xdg-dbus-proxy` |
| `notify`, `xdg_open`, `clipboard` | ask the host to show notifications, open URIs, read/write the clipboard |
| `host_exec` | run host commands as you. Without an `allowlist`, **any** command |
| `ssh_agent`, `gpg_agent` | use your keys (not read them) |
| shared XDG dirs, mounts | read, or with `rw` modify, those directories |
| `[integration.hardware]` | open the listed device nodes |

Treat a container with `host_exec` and no allowlist as having full access to
your account.

### In scope

Bugs where podbox grants more than the config says, for example:
- the host socket accepting a request whose capability wasn't negotiated or
  enabled;
- bypassing the `host_exec` allowlist, or injecting arguments or commands
  through it;
- getting past the D-Bus proxy rules or the Wayland firewall;
- path traversal or file overwrite on the host through names, config values,
  or files podbox writes;
- generated Containerfiles or Quadlet units that drop a security setting or
  add a mount or device the config didn't ask for;
- tampering with the install script or release artefacts.

### Out of scope

- Access the config grants on purpose (see the table above).
- Bugs in podman, crun, the kernel, xdg-dbus-proxy or your compositor; please
  report those upstream. We still want to hear about them if podbox makes
  them easier to exploit.
- Anything that requires being able to edit your podbox config or run
  commands as your user on the host already.
- Malicious base images or packages you chose to install.
