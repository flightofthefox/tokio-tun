use std::net::Ipv4Addr;
use std::os::unix::io::AsRawFd;
use std::sync::Arc;
use tokio_tun::Tun;

#[tokio::main]
async fn main() {
    // macOS doesn't support multi-queue, so we'll only use 1 queue on macOS
    #[cfg(target_os = "macos")]
    let queues = 1;

    #[cfg(not(target_os = "macos"))]
    let queues = 3;

    println!("Creating {} queue(s) (macOS only supports 1 queue)", queues);

    let tuns = Tun::builder()
        .name("")
        .mtu(1350)
        .up()
        .address(Ipv4Addr::new(10, 0, 0, 1))
        .destination(Ipv4Addr::new(10, 1, 0, 1))
        .broadcast(Ipv4Addr::new(10, 0, 0, 255))
        .netmask(Ipv4Addr::new(255, 255, 255, 0))
        .queues(queues)
        .build()
        .unwrap();

    println!("--------------");
    println!("{} tuns created", tuns.len());
    println!("--------------");

    // Print device information
    #[cfg(target_os = "macos")]
    {
        println!(
            "┌ name: {}\n├ fd: {}\n├ mtu: {}\n├ flags: {}\n├ address: {}\n├ destination: {}\n├ broadcast: {}\n└ netmask: {}",
            tuns[0].name(),
            tuns[0].as_raw_fd(),
            tuns[0].mtu().unwrap(),
            tuns[0].flags().unwrap(),
            tuns[0].address().unwrap(),
            tuns[0].destination().unwrap(),
            tuns[0].broadcast().unwrap(),
            tuns[0].netmask().unwrap(),
        );
    }

    #[cfg(not(target_os = "macos"))]
    {
        println!(
            "┌ name: {}\n├ fd: {}, {}, {}\n├ mtu: {}\n├ flags: {}\n├ address: {}\n├ destination: {}\n├ broadcast: {}\n└ netmask: {}",
            tuns[0].name(),
            tuns[0].as_raw_fd(),
            tuns[1].as_raw_fd(),
            tuns[2].as_raw_fd(),
            tuns[0].mtu().unwrap(),
            tuns[0].flags().unwrap(),
            tuns[0].address().unwrap(),
            tuns[0].destination().unwrap(),
            tuns[0].broadcast().unwrap(),
            tuns[0].netmask().unwrap(),
        );
    }

    #[cfg(target_os = "linux")]
    {
        println!("---------------------");
        println!("ping 10.1.0.2 to test");
        println!("---------------------");
    }

    #[cfg(target_os = "macos")]
    {
        println!("------------------------------");
        println!("ping -S 10.1.0.2 10.0.0.1 to test");
        println!("------------------------------");
    }

    // Create Arc references to each TUN device
    let mut tuns_arc = Vec::new();
    for tun in tuns {
        tuns_arc.push(Arc::new(tun));
    }

    // Create buffers for each queue
    let mut buffers = Vec::new();
    for _ in 0..tuns_arc.len() {
        buffers.push([0u8; 1024]);
    }

    // Handle different number of queues for different platforms
    #[cfg(target_os = "macos")]
    {
        let tun0 = &tuns_arc[0];
        let mut buf0 = [0u8; 1024];

        println!("Starting to listen on 1 queue...");
        loop {
            match tun0.recv(&mut buf0).await {
                Ok(n) => println!("reading {} bytes from tun: {:?}", n, &buf0[..n]),
                Err(e) => println!("Error reading: {:?}", e),
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        // For Linux and other platforms with multi-queue support
        let tun0 = Arc::clone(&tuns_arc[0]);
        let tun1 = Arc::clone(&tuns_arc[1]);
        let tun2 = Arc::clone(&tuns_arc[2]);

        let mut buf0 = [0u8; 1024];
        let mut buf1 = [0u8; 1024];
        let mut buf2 = [0u8; 1024];

        println!("Starting to listen on 3 queues...");
        loop {
            let (buf, id) = tokio::select! {
                Ok(n) = tun0.recv(&mut buf0) => (&buf0[..n], 0),
                Ok(n) = tun1.recv(&mut buf1) => (&buf1[..n], 1),
                Ok(n) = tun2.recv(&mut buf2) => (&buf2[..n], 2),
            };
            println!("reading {} bytes from tuns[{}]: {:?}", buf.len(), id, buf);
        }
    }
}
