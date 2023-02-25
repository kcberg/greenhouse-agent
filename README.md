
### Raspberry PI 32bit cross compile armv7l GNU/Linux

```shell
rustup target install armv7-unknown-linux-gnueabihf
cargo install cross
cross build --release --target armv7-unknown-linux-gnueabihf
```