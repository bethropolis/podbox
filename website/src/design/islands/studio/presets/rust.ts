import { Flame } from 'lucide-react';
import { named, type PresetDef } from './shared';

export const rustPreset: PresetDef = {
  id: 'rust',
  label: 'Rust Dev',
  blurb: 'CachyOS, mold, LazyVim',
  icon: Flame,
  accent: 'text-[var(--accent-peach)]',
  overrides: {
    selectedPresetDistro: 'cachyos/cachyos:latest',
    ...named('rust-dev'),
    // base-devel + clang: the C toolchain behind every native build.
    // mold + the mold-linker step: the default linker via cargo config.
    // `git` is explicit because the base set has curl but *not* git.
    packagesInstallList: ['base-devel', 'clang', 'mold', 'git', 'neovim', 'ripgrep', 'fd', 'mise', 'fish'],
    packageManager: 'pacman',
    containerShell: 'fish',
    // The preset compiles Rust and links through clang+mold: the crate
    // registry/git caches, the C toolchain's compiler cache and the shared
    // build cache all survive container recreation as volumes.
    sharedCaches: ['cargo', 'ccache', 'mbx'],
    provisionSteps: [
      {
        name: 'mise-activate',
        run: "mkdir -p ~/.config/fish/conf.d\necho 'mise activate fish | source' > ~/.config/fish/conf.d/mise.fish",
        timeout: '5m',
        onFailure: 'warn',
        envList: [],
        root: false,
      },
      {
        name: 'mold-linker',
        run: `mkdir -p ~/.cargo\nprintf '[target.x86_64-unknown-linux-gnu]\nlinker = "clang"\nrustflags = ["-C", "link-arg=-fuse-ld=mold"]\n' > ~/.cargo/config.toml`,
        timeout: '5m',
        onFailure: 'warn',
        envList: [],
        root: false,
      },
      {
        name: 'rust-toolchain',
        run: "mise use -g 'rust[components=rust-analyzer,clippy,rustfmt]@stable'",
        timeout: '5m',
        onFailure: 'warn',
        envList: [],
        root: false,
      },
      {
        name: 'lazyvim',
        run: '[ -d ~/.config/nvim ] || git clone https://github.com/LazyVim/starter ~/.config/nvim',
        timeout: '5m',
        onFailure: 'warn',
        envList: [],
        root: false,
      },
    ],
    // Everything below is already the Studio default except the opt-ins,
    // which are the only lines that reach the TOML.
    intXdgOpen: true,
  },
};
