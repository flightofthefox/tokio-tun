use std::convert::From;
use std::io::{self, IoSlice, Read, Write};
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};

pub struct TunIo(RawFd);

impl From<RawFd> for TunIo {
    fn from(fd: RawFd) -> Self {
        Self(fd)
    }
}

impl FromRawFd for TunIo {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        Self(fd)
    }
}

impl AsRawFd for TunIo {
    fn as_raw_fd(&self) -> RawFd {
        self.0
    }
}

impl Read for TunIo {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.recv(buf)
    }
}

impl Write for TunIo {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.send(buf)
    }

    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        self.sendv(bufs)
    }

    fn flush(&mut self) -> io::Result<()> {
        let ret = unsafe { libc::fsync(self.0) };
        if ret < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

impl TunIo {
    pub fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        // macOS utun adds a 4-byte header to each packet
        // First 4 bytes are family type (AF_INET, AF_INET6)
        let mut vec = vec![0u8; buf.len() + 4];
        let n = unsafe { libc::read(self.0, vec.as_mut_ptr() as *mut _, vec.len() as _) };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }

        if n < 4 {
            return Ok(0);
        }

        let data_size = n as usize - 4;
        buf[..data_size].copy_from_slice(&vec[4..n as usize]);
        Ok(data_size)
    }

    pub fn send(&self, buf: &[u8]) -> io::Result<usize> {
        // Prepend 4-byte header
        // For IPv4, the value is 2 (AF_INET) in network byte order
        let mut vec = vec![0, 0, 0, 2];
        vec.extend_from_slice(buf);

        let n = unsafe { libc::write(self.0, vec.as_ptr() as *const _, vec.len() as _) };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }

        if n <= 4 {
            return Ok(0);
        }

        Ok(n as usize - 4)
    }

    pub fn sendv(&self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        // For the macOS implementation, we need to handle the 4-byte header
        // Since we can't easily modify IoSlice, we'll convert to a continuous buffer
        let mut data = Vec::new();
        // Add the 4-byte protocol header (AF_INET = 2 in network byte order)
        data.extend_from_slice(&[0, 0, 0, 2]);

        for buf in bufs {
            data.extend_from_slice(buf);
        }

        let n = unsafe { libc::write(self.0, data.as_ptr() as *const _, data.len() as _) };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }

        if n <= 4 {
            return Ok(0);
        }

        Ok(n as usize - 4)
    }
}

impl Drop for TunIo {
    fn drop(&mut self) {
        unsafe { libc::close(self.0) };
    }
}
