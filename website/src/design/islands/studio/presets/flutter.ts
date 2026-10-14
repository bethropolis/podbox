import { Smartphone } from 'lucide-react';
import { headlessIntegration, named, type PresetDef } from './shared';

// Pinned Android command-line tools; sdkmanager updates itself afterwards.
const CMDLINE_TOOLS = 'https://dl.google.com/android/repository/commandlinetools-linux-11076708_latest.zip';

export const flutterPreset: PresetDef = {
  id: 'flutter',
  label: 'Flutter Dev',
  blurb: 'Slim, Android over Wi-Fi',
  icon: Smartphone,
  accent: 'text-[var(--accent-teal)]',
  overrides: {
    selectedPresetDistro: 'archlinux:latest',
    ...named('flutter'),
    // Kept slim: no desktop toolchain, browser or fonts. The phone is reached
    // with `adb pair` / `adb connect` over Wi-Fi, and web builds are served to
    // the host browser with `flutter run -d web-server --web-port 8080`.
    // What remains is what the SDK scripts shell out to, plus JDK 17 for Gradle.
    packagesInstallList: ['git', 'curl', 'unzip', 'zip', 'xz', 'which', 'jdk17-openjdk', 'fish'],
    containerShell: 'fish',
    provisionSteps: [
      {
        // A full clone, not --depth 1: the tool reads its version from git tags.
        name: 'flutter-sdk',
        run: '[ -d ~/.local/flutter ] || git clone -b stable https://github.com/flutter/flutter.git ~/.local/flutter\nf=~/.local/flutter/bin/flutter\n$f config --no-analytics --no-enable-linux-desktop\n$f precache --android --web',
        timeout: '20m',
        onFailure: 'abort',
        envList: [],
        root: false,
      },
      {
        // Only cmdline-tools and platform-tools (adb) up front: Gradle fetches
        // the platform and build-tools each project asks for on first build.
        // Accepts the Android SDK licences on your behalf.
        name: 'android-sdk',
        run: `export ANDROID_HOME=~/Android/Sdk\nif [ ! -d "$ANDROID_HOME/cmdline-tools/latest" ]; then\n  mkdir -p "$ANDROID_HOME/cmdline-tools"\n  curl -fsSL -o /tmp/cmdline-tools.zip ${CMDLINE_TOOLS}\n  unzip -q /tmp/cmdline-tools.zip -d "$ANDROID_HOME/cmdline-tools"\n  mv "$ANDROID_HOME/cmdline-tools/cmdline-tools" "$ANDROID_HOME/cmdline-tools/latest"\n  rm -f /tmp/cmdline-tools.zip\nfi\nsdk="$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager"\nyes | "$sdk" --licenses > /dev/null\n"$sdk" platform-tools\n~/.local/flutter/bin/flutter config --android-sdk "$ANDROID_HOME"`,
        timeout: '20m',
        onFailure: 'abort',
        envList: [],
        root: false,
      },
      {
        name: 'shell-path',
        run: "mkdir -p ~/.config/fish/conf.d\nprintf '%s\\n' 'set -gx ANDROID_HOME ~/Android/Sdk' 'fish_add_path ~/.local/flutter/bin ~/.pub-cache/bin $ANDROID_HOME/cmdline-tools/latest/bin $ANDROID_HOME/platform-tools' > ~/.config/fish/conf.d/flutter.fish",
        timeout: '5m',
        onFailure: 'warn',
        envList: [],
        root: false,
      },
    ],
    // The SDK, pub cache and Android SDK live in the home volume already;
    // Gradle's cache is the one worth sharing across boxes.
    sharedCaches: ['gradle'],
    portMappingsList: ['8080:8080'],
    // Nothing here opens a window.
    ...headlessIntegration,
    intGitIdentity: true,
    intSshAgent: true,
    intClipboard: true,
    intXdgOpen: true,
    lifeQuadlet: true,
  },
};
