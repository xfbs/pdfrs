# Building the Android app

## Prerequisites

- Rust 1.92 or newer, installed with [rustup](https://rustup.rs), with the
  Android target and cargo-ndk:

  ```sh
  rustup target add aarch64-linux-android
  cargo install cargo-ndk
  ```

- Android Studio, or the Android SDK on its own, with the NDK version that
  `build.gradle.kts` names as `ndkVersion`. In Android Studio's settings it is
  under Languages & Frameworks, Android SDK, SDK Tools, with "Show Package
  Details" ticked.
- A JDK. Android Studio brings one. To use it from a terminal on macOS:

  ```sh
  export JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home"
  export ANDROID_HOME=~/Library/Android/sdk
  ```

Gradle cross-compiles the Rust library and generates the Kotlin bindings as
part of the build, so there is no separate Rust step. The app runs on Android
8.0 (API 26) and newer.

## Build and install

From `apps/android`, with a phone connected or an emulator running:

```sh
# build and install
./gradlew installDebug

# only build, into build/outputs/apk/debug/
./gradlew assembleDebug
```

With more than one device connected, say which one:

```sh
adb devices
ANDROID_SERIAL=emulator-5554 ./gradlew installDebug
```

## Release build

The release build is the one to judge speed by, and the one users get. Its code
is shrunk by R8, which removes what the app doesn't use.

```sh
./gradlew installRelease
```

Without a key of its own (see below) it is signed with the debug key, which is
fine for trying it.

The UI tests run against the debug build, which isn't shrunk. What R8 must not
touch is listed in `proguard-rules.pro`. After changing that file, or how
Kotlin reaches the Rust library, open a document in the release build and see
that it draws.

## A release for others

A build to hand out has to be signed with a key that stays the same for as long
as the app exists, because Android installs an update only if it is signed with
the key of the version it replaces. Create one once, and keep the file and its
password somewhere safe, outside the repository:

```sh
keytool -genkeypair -v -keystore ~/specto-release.jks -alias specto \
    -keyalg RSA -keysize 4096 -validity 10000
```

Then say where it is in `apps/android/keystore.properties`, which Git ignores:

```properties
storeFile=/home/me/specto-release.jks
storePassword=<password>
keyAlias=specto
keyPassword=<password>
```

With that file in place, the release build is signed with the key. Without it,
the debug key is used.

```sh
# an APK, in build/outputs/apk/release/
./gradlew assembleRelease

# an app bundle, for stores that want one
./gradlew bundleRelease
```

To see which key signed an APK:

```sh
$ANDROID_HOME/build-tools/<version>/apksigner verify --print-certs specto-release.apk
```

The version comes from the file `VERSION` at the root of the repository, which
the desktop app shares. Android takes an update only with a higher version.

## Publishing a release

Pushing a tag that starts with "v" makes CI build the release APK, sign it with
the release key and attach it to a new release on GitHub, together with its
SHA-256 checksum. The workflow is `.github/workflows/release.yml`, which builds
the desktop app for the same release. The steps of a release are in
[packaging/README.md](../../packaging/README.md).

CI needs the key once, as secrets of the repository on GitHub (Settings,
"Secrets and variables", Actions):

- `ANDROID_KEYSTORE`: the key file, encoded as Base64 (see below)
- `ANDROID_KEYSTORE_PASSWORD`: the password of the file
- `ANDROID_KEY_ALIAS`: the name of the key in it, such as "specto"
- `ANDROID_KEY_PASSWORD`: the password of the key

```sh
# the key file as one line of Base64
base64 < ~/specto-release.jks | tr -d '\n'
```

The workflow stops without publishing anything if the tag and `VERSION`
differ, if the changelog or the metainfo has no entry for the version, if a
secret is missing, or if the APK turns out to be signed with a debug key.

The APK is for arm64-v8a, which every phone and tablet with a 64-bit processor
runs.

## Emulators on Intel and AMD machines

The Rust library is built for arm64-v8a, which covers phones and the emulator
on Apple Silicon. An emulator on an Intel or AMD machine needs x86_64:

```sh
rustup target add x86_64-linux-android
./gradlew installDebug -Pspecto.abis=x86_64
```

To build both, pass both, or change `specto.abis` in `gradle.properties`:

```sh
./gradlew installDebug -Pspecto.abis=arm64-v8a,x86_64
```

## Tests

```sh
# unit tests
./gradlew testDebugUnitTest

# UI tests on an emulator
ANDROID_SERIAL=emulator-5554 ./gradlew connectedDebugAndroidTest

# only some UI test classes
ANDROID_SERIAL=emulator-5554 ./gradlew connectedDebugAndroidTest \
    -Pandroid.testInstrumentationRunnerArguments.class=net.xfbs.specto.EditTest,net.xfbs.specto.ShareTest
```

Set the serial when a phone is plugged in as well, because a UI test run
installs and then removes the app on its target. What the tests cover, and what
is checked by hand, is in [TESTING.md](TESTING.md).

## Code style

ktlint formats the Kotlin code and the build script, by the rules in
`.editorconfig`: lines of at most 100 columns and a comma after the last item
of a list that runs over several lines. CI fails on code that isn't formatted.

```sh
# format everything, and report what can't be fixed automatically
./gradlew ktlintFormat

# only check
./gradlew ktlintCheck
```

## When something is off

- After the Rust library or its bindings were renamed or removed, clean the
  build. Generated files from before are otherwise packaged next to the new
  ones.

  ```sh
  ./gradlew clean
  ```

- The first install on a freshly started emulator sometimes fails, and a second
  try works.
