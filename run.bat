@echo off
rem Build (if needed) and launch TreeCraft. Double-click this file to run the app.
cargo run --manifest-path "%~dp0app\Cargo.toml" %*
