# TODO's
Search the codebase for `TODO`.

# Motivation for this plugin
I want to make a game for Android and iOS using the bevy game engine.
I ran into issues with winit on both platforms that I couldn't resolve or ignore.

## Issues on Android
- https://github.com/rust-windowing/winit/issues/3325

## Issues on iOS
- https://github.com/rust-windowing/winit/issues/4224
- https://github.com/rust-windowing/winit/issues/4601

# Build

## Android
- Follow the guide on https://github.com/libsdl-org/SDL/blob/main/docs/README-android.md
- Build lib with `crate-type = ["cdylib"]`
- Add the lib to the jniLibs/<architecture> folder

## iOS
- Follow the guide on https://github.com/libsdl-org/SDL/blob/main/docs/README-ios.md
- Build lib with `crate-type = ["staticlib"]`
- In the xcode project select the project in the main view, go to the "General" tab, scroll down to
  "Frameworks, Libraries, and Embedded Content", and drag and drop the `.a` lib.
- Still in "Frameworks, Libraries, and Embedded Content", select "Embed & Sign" for the `.a` lib.

# Example
Run:
```sh
cargo run --example hello_world
```
