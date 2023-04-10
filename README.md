## Requirements

* [rust](https://www.rust-lang.org/tools/install) This is a rust project. ![](https://www.rust-lang.org/logos/rust-logo-32x32.png)
  * [cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html) - The built-in build toolchain for rust.
* [cross](https://github.com/cross-rs/cross#installation) - A tool to help cross compilation to armv7 and aarch64 for raspberrypi models.
* [cargo-make](https://sagiegurari.github.io/cargo-make/) - A task oriented build tool.

```shell
cargo install cross
cargo install --force cargo-make
```

### Raspberry PI 32bit cross compile armv7l GNU/Linux

```shell
rustup target install armv7-unknown-linux-gnueabihf
cargo make --profile production build_armv7_with_ui
```

### Raspberry PI 64bit cross compile aarch64 GNU/Linux

```shell
rustup target install aarch64-unknown-linux-gnu
cargo make --profile production build_aarch64_with_ui 
```

