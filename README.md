# State
Currently tries to match bevy_winit up to commit:
https://github.com/bevyengine/bevy/commit/cdffa0dd2a387eb76f9c92bebaccee96f308d287



# TODO's
Search the codebase for `TODO`.



# Motivation for this plugin
I want to make a game for Android and iOS using the bevy game engine.
I ran into issues with winit on both platforms, and on iOS I was unable to resolve or work around
them. For now, my plan is to use this plugin on iOS and the default winit plugin on PC and Android.

## winit issues on Android
https://github.com/bevyengine/bevy/issues/26282

## winit issues on iOS
- https://github.com/rust-windowing/winit/issues/4224
- https://github.com/rust-windowing/winit/issues/4601

iOS might also have some of the performance issues from Android:
https://github.com/bevyengine/bevy/issues/26282



# Build

## PC
- See https://github.com/vhspace/sdl3-rs

## iOS
1. The extern fn impl in rust `src/lib.rs`:
```rust
#[unsafe(no_mangle)]
pub extern "C" fn run_app() {
    // App::new() stuff with Sdl3Plugin see example hello_world
}
```
2. Build lib with `crate-type = ["staticlib"]`
3. Follow the guide on https://github.com/libsdl-org/SDL/blob/main/docs/README-ios.md
4. You should now have an xcode project.
5. In the xcode project select the project in the main view, go to the "General" tab, scroll down to
   "Frameworks, Libraries, and Embedded Content", and drag and drop the `.a` lib.
6. Still in "Frameworks, Libraries, and Embedded Content", select "Embed & Sign" for the `.a` lib.
7. I also had to link `AudioToolbox.framework`, `Metal.framework` and `UIKit.framework`
8. Add a `main.m` file:
```objc
#include <SDL3/SDL_main.h>

extern void run_app(void);

int main(int argc, char *argv[]) {
    run_app();
    return 0;
}
```

### iOS xcode diagnostics memory leak
I noticed that the memory usage of the app kept increasing significantly over time. I was able to
fix the issue by disabling `Metal API Validation` in the xcode scheme settings.
1. Product
2. Scheme
3. Edit Scheme
4. Run
5. Diagnostics
6. Disable `Metal API Validation`

---

## Android
1. Set the envs (see below)
2. Follow the guide on https://github.com/libsdl-org/SDL/blob/main/docs/README-android.md
3. Build lib with `crate-type = ["cdylib"]`
4. I think features = ["build-from-source"] on sdl3 create is needed.
5. Add the `lib<name>.so` and the `libSDL3.so` to the `jniLibs/<architecture>` folder

Envs for arm64:
- Replace:
    - <ndk_version>
    - <your_os>
    - <min_sdk_version>
```sh
export ANDROID_SDK_ROOT="$HOME/Android/Sdk"
export ANDROID_NDK_ROOT="$ANDROID_SDK_ROOT/ndk/<ndk_version>"
export CMAKE_TOOLCHAIN_FILE_aarch64_linux_android="$ANDROID_NDK_ROOT/build/cmake/android.toolchain.cmake"
export CC_aarch64_linux_android="$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/<your_os>/bin/aarch64-linux-android<min_sdk_version>-clang"
export CXX_aarch64_linux_android="$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/<your_os>/bin/aarch64-linux-android<min_sdk_version>-clang++"
export AR_aarch64_linux_android="$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/<your_os>/bin/llvm-ar"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/<your_os>/bin/aarch64-linux-android<min_sdk_version>-clang"
```

### Android issues with this plugin
- Bevy relies on AndroidApp from android-activity crate to access the AssetManager.
- wgpu loggs errors because of surface destruction and sdl AppWillEnterBackground is received after
  them so I don't know how to fix this.



# Mobile `WindowPlugin` config
```rust
DefaultPlugins.set(WindowPlugin {
    primary_window: Some(Window {
        present_mode: PresentMode::AutoVsync, // Or PresentMode::Fifo

        #[cfg(any(target_os = "android", target_os = "ios"))]
        mode: bevy::window::WindowMode::BorderlessFullscreen(
            bevy::window::MonitorSelection::Current,
        ),

        ..default()
    }),
    ..default()
})
```



# Example
Run:
```sh
cargo run --example hello_world
```
