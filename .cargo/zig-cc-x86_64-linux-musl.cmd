@echo off
zig cc -target x86_64-linux-musl -nostartfiles %*
