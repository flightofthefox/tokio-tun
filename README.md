# Tokio TUN/TAP

[![Build](https://github.com/yaa110/tokio-tun/workflows/Build/badge.svg)](https://github.com/yaa110/tokio-tun/actions) [![crates.io](https://img.shields.io/crates/v/tokio-tun.svg)](https://crates.io/crates/tokio-tun) [![Documentation](https://img.shields.io/badge/docs-tokio--tun-blue.svg)](https://docs.rs/tokio-tun) [![examples](https://img.shields.io/badge/examples-tokio--tun-blue.svg)](examples)

Asynchronous allocation of TUN/TAP devices in Rust using [`tokio`](https://crates.io/crates/tokio). Use [async-tun](https://crates.io/crates/async-tun) for `async-std` version.

## Getting Started

- Create a tun device using `Tun::builder()` and read from it in a loop:

```rust
#[tokio::main]
async fn main() {
    let tun = Arc::new(
        Tun::builder()
            .name("")            // if name is empty, then it is set by kernel.
            .tap()               // uses TAP instead of TUN (default).
            .packet_info()       // avoids setting IFF_NO_PI.
            .up()                // or set it up manually using `sudo ip link set <tun-name> up`.
            .build()
            .unwrap()
            .pop()
            .unwrap(),
    );

    println!("tun created, name: {}, fd: {}", tun.name(), tun.as_raw_fd());

    let (mut reader, mut _writer) = tokio::io::split(tun);

    // Writer: simply clone Arced Tun.
    let tun_c = tun.clone();
    tokio::spawn(async move{
        let buf = b"data to be written";
        tun_c.send_all(buf).await.unwrap();
    });

    // Reader
    let mut buf = [0u8; 1024];
    loop {
        let n = tun.recv(&mut buf).await.unwrap();
        println!("reading {} bytes: {:?}", n, &buf[..n]);
    }
}
```

- Run the code using `sudo`:

```bash
sudo -E $(which cargo) run
```

### Linux

- Set the address of device (address and netmask could also be set using `TunBuilder`):

```bash
sudo ip a add 10.0.0.1/24 dev <tun-name>
```

- Ping to read packets:

```bash
ping 10.0.0.2
```

- Display devices and analyze the network traffic:

```bash
ip tuntap
sudo tshark -i <tun-name>
```

### macOS

On macOS, the library uses the `utun` interface which is part of the macOS kernel. You need to have root privileges to create and use utun devices.

- The name parameter for macOS should be in the format of `utun[0-9]+` (e.g., "utun0", "utun1") or empty to let the kernel assign the next available utun device.

- Set the address of device (address and netmask could also be set using `TunBuilder`):

```bash
sudo ifconfig <utun-name> 10.0.0.1 10.1.0.1 netmask 255.255.255.0 up
```

- Ping to read packets:

```bash
ping -S 10.1.0.2 10.0.0.1
```

- Display devices:

```bash
ifconfig | grep utun
```

## Supported Platforms

- [x] Linux
- [x] macOS (via utun interface)
- [ ] FreeBSD
- [ ] Android
- [ ] iOS
- [ ] Windows

## Examples

- [`read`](examples/read.rs): Split tun to (reader, writer) pair and read packets from reader.
- [`read-mq`](examples/read-mq.rs): Read from multi-queue tun using `tokio::select!`.
- [`cross_platform`](examples/cross_platform.rs): Example showing cross-platform usage on both Linux and macOS.

```bash
sudo -E $(which cargo) run --example read
sudo -E $(which cargo) run --example read-mq
sudo -E $(which cargo) run --example cross_platform
```

## Platform-specific Notes

### macOS

The macOS implementation uses the `utun` interface which has a few differences from the Linux TUN/TAP implementation:

1. macOS adds a 4-byte header to each packet (2 bytes for address family)
2. Multi-queue is not supported on macOS
3. TAP mode simulates Ethernet frames but behaves differently than Linux TAP devices
4. The utun interfaces in macOS are point-to-point interfaces, so broadcast addresses behave differently. The library has been adapted to handle this difference transparently.

### Linux

Linux supports both TUN and TAP devices with full feature set including multi-queue support.
