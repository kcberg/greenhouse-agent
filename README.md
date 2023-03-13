
## Requirements

```shell
cargo install cross
```

### Raspberry PI 32bit cross compile armv7l GNU/Linux

```shell
rustup target install armv7-unknown-linux-gnueabihf
cross build --release --target armv7-unknown-linux-gnueabihf
```

### Raspberry PI 64bit cross compile aarch64 GNU/Linux

```shell
rustup target install aarch64-unknown-linux-gnu
cross build --release --target aarch64-unknown-linux-gnu
```

