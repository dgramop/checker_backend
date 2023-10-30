echo '[target.x86_64-unknown-linux-gnu]
linker = "x86_64-unknown-linux-gnu-gcc"' > .cargo/config

TARGET_CC="x86_64-unknown-linux-gnu-gcc" cargo build --target x86_64-unknown-linux-gnu

rm .cargo/config

