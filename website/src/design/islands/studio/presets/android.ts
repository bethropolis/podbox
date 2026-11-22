import { Smartphone } from 'lucide-react';
import { named, type PresetDef } from './shared';

// Android tooling is installed through mise. The SDK lives under
// `$HOME/Android/Sdk` everywhere, so no host username or absolute home path
// is baked into the preset; the fish config and every provision step derive
// it from `$HOME` inside the container.
export const androidPreset: PresetDef = {
  id: 'android',
  label: 'Android Dev',
  blurb: 'SDK, adb, device mirroring',
  icon: Smartphone,
  accent: 'text-[var(--accent-teal)]',
  overrides: {
    selectedPresetDistro: 'cachyos/cachyos:latest',
    ...named('android'),
    packagesInstallList: [
      'ca-certificates',
      'fish',
      'mise',
      'git',
      'base-devel',
      'curl',
      'zip',
      'unzip',
      'jq',
      'ripgrep',
      'fd',
      'file',
      'less',
      'fastfetch',
      'btop',
      'openssh',
    ],
    packageManager: 'pacman',
    containerShell: 'fish',
    provisionSteps: [
      {
        name: 'shell-env',
        run: [
          'set -eu',
          'mkdir -p "$HOME/.config/fish/conf.d"',
          "printf '%s\\n' 'mise activate fish | source' > \"$HOME/.config/fish/conf.d/mise.fish\"",
          "printf '%s\\n' 'set -gx ANDROID_HOME $HOME/Android/Sdk' 'fish_add_path $ANDROID_HOME/platform-tools $ANDROID_HOME/cmdline-tools/latest/bin' > \"$HOME/.config/fish/conf.d/android-sdk.fish\"",
        ].join('\n'),
        verify:
          'test -s "$HOME/.config/fish/conf.d/mise.fish" && test -s "$HOME/.config/fish/conf.d/android-sdk.fish"',
        timeout: '5m',
        onFailure: 'abort',
        envList: [],
        root: false,
      },
      {
        name: 'android-toolchain',
        run: [
          'set -eu',
          'mise use --global java@temurin-25.0.5+8.0.LTS',
          'mise use --global android-cli@1.1.16717390',
          'mise exec -- java -version',
          'mise exec -- javac -version',
          'mise exec -- android --version',
        ].join('\n'),
        verify:
          'mise exec -- java -version && mise exec -- javac -version && mise exec -- android --version',
        timeout: '30m',
        onFailure: 'abort',
        envList: [],
        root: false,
      },
      {
        name: 'android-sdk-config',
        run: [
          'set -eu',
          'export ANDROID_HOME="$HOME/Android/Sdk"',
          'mkdir -p "$ANDROID_HOME" "$HOME/.android"',
          "printf '%s\\n' \"--sdk=$ANDROID_HOME\" --no-metrics > \"$HOME/.androidrc\"",
        ].join('\n'),
        verify:
          'export ANDROID_HOME="$HOME/Android/Sdk" && test -d "$ANDROID_HOME" && grep -Fx -- "--sdk=$ANDROID_HOME" "$HOME/.androidrc" && grep -Fx -- "--no-metrics" "$HOME/.androidrc"',
        timeout: '5m',
        onFailure: 'abort',
        envList: [],
        root: false,
      },
      {
        name: 'android-sdk',
        run: [
          'set -eu',
          'export ANDROID_HOME="$HOME/Android/Sdk"',
          'mise exec -- android sdk install platform-tools platforms/android-37.2 build-tools/36.0.0',
        ].join('\n'),
        verify:
          'export ANDROID_HOME="$HOME/Android/Sdk" && test -x "$ANDROID_HOME/platform-tools/adb" && test -d "$ANDROID_HOME/platforms/android-37.2" && test -d "$ANDROID_HOME/build-tools/36.0.0"',
        timeout: '45m',
        onFailure: 'abort',
        envList: [],
        root: false,
      },
    ],
    // Gradle caches dominate a rebuild; the SDK itself already lives in the
    // home volume.
    sharedCaches: ['gradle'],
    portMappingsList: ['8000:8000'],
    intXdgOpen: true,
    intClipboard: true,
    intSshAgent: true,
    intSyncFonts: true,
    intSyncIcons: true,
    intSyncThemes: true,
    // `host-adb` rather than `adb`: the container has its own adb from
    // platform-tools, and the shim must not shadow it. Host adb and scrcpy
    // work against the USB device passed through below.
    hostExecEnabled: true,
    hostExecList: [
      { alias: 'gh', path: '/usr/bin/gh', filter: true },
      { alias: 'git', path: '/usr/bin/git', filter: true },
      { alias: 'host-adb', path: '/usr/bin/adb', filter: true },
      { alias: 'scrcpy', path: '/usr/bin/scrcpy', filter: true },
    ],
    hwUsb: true,
    // Defaults, kept explicit because they are part of the preset's point:
    // Projects is writable so Gradle and adb can see it, and the keep-id
    // userns lets the host user manipulate USB devices.
    xdgProjects: 'rw',
    noNewPrivileges: false,
    usernsMode: 'keep-id',
    lifeQuadlet: true,
  },
};
