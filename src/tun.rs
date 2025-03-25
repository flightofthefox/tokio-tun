use crate::Result;
use crate::TunBuilder;
#[cfg(target_os = "linux")]
use crate::linux::interface::Interface;
#[cfg(target_os = "linux")]
use crate::linux::io::TunIo;
#[cfg(target_os = "linux")]
use crate::linux::params::Params;
#[cfg(target_os = "macos")]
use crate::macos::interface::Interface;
#[cfg(target_os = "macos")]
use crate::macos::io::TunIo;
#[cfg(target_os = "macos")]
use crate::macos::params::Params;
use std::io::{self, ErrorKind, IoSlice, Read, Write};
use std::mem;
use std::net::Ipv4Addr;
#[cfg(target_os = "linux")]
use std::os::raw::c_char;
use std::os::unix::io::{AsRawFd, RawFd};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{self, Context, Poll};
use tokio::io::unix::AsyncFd;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

#[cfg(target_os = "linux")]
static TUN: &[u8] = b"/dev/net/tun\0";

// Taken from the `futures` crate
macro_rules! ready {
    ($e:expr $(,)?) => {
        match $e {
            std::task::Poll::Ready(t) => t,
            std::task::Poll::Pending => return std::task::Poll::Pending,
        }
    };
}

/// Represents a Tun/Tap device. Use [`TunBuilder`](struct.TunBuilder.html) to create a new instance of [`Tun`](struct.Tun.html).
pub struct Tun {
    iface: Arc<Interface>,
    io: AsyncFd<TunIo>,
}

impl AsRawFd for Tun {
    fn as_raw_fd(&self) -> RawFd {
        self.io.as_raw_fd()
    }
}

impl AsyncRead for Tun {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> task::Poll<io::Result<()>> {
        let self_mut = self.get_mut();
        loop {
            let mut guard = ready!(self_mut.io.poll_read_ready_mut(cx))?;

            match guard.try_io(|inner| inner.get_mut().read(buf.initialize_unfilled())) {
                Ok(Ok(n)) => {
                    buf.set_filled(buf.filled().len() + n);
                    return Poll::Ready(Ok(()));
                }
                Ok(Err(err)) => return Poll::Ready(Err(err)),
                Err(_) => continue,
            }
        }
    }
}

impl AsyncWrite for Tun {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> task::Poll<io::Result<usize>> {
        let self_mut = self.get_mut();
        loop {
            let mut guard = ready!(self_mut.io.poll_write_ready_mut(cx))?;

            match guard.try_io(|inner| inner.get_mut().write(buf)) {
                Ok(result) => return Poll::Ready(result),
                Err(_would_block) => continue,
            }
        }
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<std::result::Result<usize, io::Error>> {
        let self_mut = self.get_mut();
        loop {
            let mut guard = ready!(self_mut.io.poll_write_ready_mut(cx))?;

            match guard.try_io(|inner| inner.get_mut().write_vectored(bufs)) {
                Ok(result) => return Poll::Ready(result),
                Err(_would_block) => continue,
            }
        }
    }

    fn is_write_vectored(&self) -> bool {
        true
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> task::Poll<io::Result<()>> {
        let self_mut = self.get_mut();
        loop {
            let mut guard = ready!(self_mut.io.poll_write_ready_mut(cx))?;

            match guard.try_io(|inner| inner.get_mut().flush()) {
                Ok(result) => return Poll::Ready(result),
                Err(_) => continue,
            }
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> task::Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

impl Tun {
    pub fn builder() -> TunBuilder {
        TunBuilder::new()
    }

    /// Creates a new instance of Tun/Tap device.
    pub(crate) fn new(params: Params) -> Result<Self> {
        let iface = Self::allocate(params, 1)?;
        let fd = iface.files()[0];
        Ok(Self {
            iface: Arc::new(iface),
            io: AsyncFd::new(TunIo::from(fd))?,
        })
    }

    /// Creates a new instance of Tun/Tap device.
    pub(crate) fn new_mq(params: Params, queues: usize) -> Result<Vec<Self>> {
        let iface = Self::allocate(params, queues)?;
        let mut tuns = Vec::with_capacity(queues);
        let iface = Arc::new(iface);
        for &fd in iface.files() {
            tuns.push(Self {
                iface: iface.clone(),
                io: AsyncFd::new(TunIo::from(fd))?,
            })
        }
        Ok(tuns)
    }

    #[cfg(target_os = "linux")]
    fn allocate(params: Params, queues: usize) -> Result<Interface> {
        let fds = (0..queues)
            .map(|_| unsafe {
                match libc::open(
                    TUN.as_ptr().cast::<c_char>(),
                    libc::O_RDWR | libc::O_NONBLOCK,
                ) {
                    fd if fd >= 0 => Ok(fd),
                    _ => Err(io::Error::last_os_error().into()),
                }
            })
            .collect::<Result<Vec<_>>>()?;

        let iface = Interface::new(
            fds,
            params.name.as_deref().unwrap_or_default(),
            params.flags,
        )?;
        iface.init(params)?;
        Ok(iface)
    }

    #[cfg(target_os = "macos")]
    fn allocate(params: Params, queues: usize) -> Result<Interface> {
        // In macOS, we use the utun interface
        let mut fds = Vec::with_capacity(queues);
        let specified_unit = if let Some(name) = &params.name {
            // Check if name is in utun format
            if name.starts_with("utun") {
                name[4..].parse::<i32>().ok()
            } else {
                None
            }
        } else {
            None
        };

        // If a specific utun name was requested, try to open it
        if let Some(unit) = specified_unit {
            let (fd, name) = Interface::open_utun(unit)?;
            fds.push(fd);

            // Create Interface instance
            let iface = Interface::new(fds, &name, params.flags)?;
            iface.init(params)?;
            return Ok(iface);
        } else {
            // Otherwise, try to open the next available utun device
            for i in 0..16 {
                // Try to open utun devices from 0 to 15
                match Interface::open_utun(i) {
                    Ok((fd, name)) => {
                        fds.push(fd);

                        // Set fd to non-blocking mode
                        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
                        if flags < 0 {
                            return Err(io::Error::last_os_error().into());
                        }

                        if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
                            return Err(io::Error::last_os_error().into());
                        }

                        // Create Interface instance
                        let iface = Interface::new(fds, &name, params.flags)?;
                        iface.init(params)?;
                        return Ok(iface);
                    }
                    Err(_) => continue,
                }
            }

            return Err(
                io::Error::new(io::ErrorKind::NotFound, "No available utun device found").into(),
            );
        }
    }

    /// Receives a packet from the Tun/Tap interface.
    ///
    /// This method takes &self, so it is possible to call this method concurrently with other methods on this struct.
    pub async fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            let mut guard = self.io.readable().await?;
            match guard.try_io(|inner| inner.get_ref().recv(buf)) {
                Ok(res) => return res,
                Err(_) => continue,
            }
        }
    }

    /// Sends a buffer to the Tun/Tap interface. Returns the number of bytes written to the device.
    ///
    /// This method takes &self, so it is possible to call this method concurrently with other methods on this struct.
    pub async fn send(&self, buf: &[u8]) -> io::Result<usize> {
        loop {
            let mut guard = self.io.writable().await?;
            match guard.try_io(|inner| inner.get_ref().send(buf)) {
                Ok(res) => return res,
                Err(_) => continue,
            }
        }
    }

    /// Sends all of a buffer to the Tun/Tap interface.
    ///
    /// This method takes &self, so it is possible to call this method concurrently with other methods on this struct.
    pub async fn send_all(&self, buf: &[u8]) -> io::Result<()> {
        let mut remaining = buf;
        while !remaining.is_empty() {
            match self.send(remaining).await? {
                0 => return Err(ErrorKind::WriteZero.into()),
                n => {
                    let (_, rest) = mem::take(&mut remaining).split_at(n);
                    remaining = rest;
                }
            }
        }
        Ok(())
    }

    /// Sends several different buffers to the Tun/Tap interface. Returns the number of bytes written to the device.
    ///
    /// This method takes &self, so it is possible to call this method concurrently with other methods on this struct.
    pub async fn send_vectored(&self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        loop {
            let mut guard = self.io.writable().await?;
            match guard.try_io(|inner| inner.get_ref().sendv(bufs)) {
                Ok(res) => return res,
                Err(_) => continue,
            }
        }
    }

    /// Tries to receive a buffer from the Tun/Tap interface.
    ///
    /// When there is no pending data, `Err(io::ErrorKind::WouldBlock)` is returned.
    ///
    /// This method takes &self, so it is possible to call this method concurrently with other methods on this struct.
    pub fn try_recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.io.get_ref().recv(buf)
    }

    /// Tries to send a packet to the Tun/Tap interface.
    ///
    /// When the socket buffer is full, `Err(io::ErrorKind::WouldBlock)` is returned.
    ///
    /// This method takes &self, so it is possible to call this method concurrently with other methods on this struct.
    pub fn try_send(&self, buf: &[u8]) -> io::Result<usize> {
        self.io.get_ref().send(buf)
    }

    /// Tries to send several different buffers to the Tun/Tap interface.
    ///
    /// When the socket buffer is full, `Err(io::ErrorKind::WouldBlock)` is returned.
    ///
    /// This method takes &self, so it is possible to call this method concurrently with other methods on this struct.
    pub fn try_send_vectored(&self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        self.io.get_ref().sendv(bufs)
    }

    /// Returns the name of Tun/Tap device.
    pub fn name(&self) -> &str {
        self.iface.name()
    }

    /// Returns the value of MTU.
    pub fn mtu(&self) -> Result<i32> {
        self.iface.mtu(None)
    }

    /// Returns the IPv4 address of MTU.
    pub fn address(&self) -> Result<Ipv4Addr> {
        self.iface.address(None)
    }

    /// Returns the IPv4 destination address of MTU.
    pub fn destination(&self) -> Result<Ipv4Addr> {
        self.iface.destination(None)
    }

    /// Returns the IPv4 broadcast address of MTU.
    pub fn broadcast(&self) -> Result<Ipv4Addr> {
        self.iface.broadcast(None)
    }

    /// Returns the IPv4 netmask address of MTU.
    pub fn netmask(&self) -> Result<Ipv4Addr> {
        self.iface.netmask(None)
    }

    /// Returns the flags of MTU.
    pub fn flags(&self) -> Result<i16> {
        self.iface.flags(None)
    }
}
