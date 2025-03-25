use super::params::Params;
use super::request::ifreq;
use crate::Result;
use crate::macos::address::Ipv4AddrExt;
use std::ffi::CString;
use std::mem;
use std::net::Ipv4Addr;
use std::os::raw::{c_char, c_int, c_void};
use std::ptr;

// Constants for macOS system calls
const CTLIOCGINFO: u64 = 0xc0644e03;
const UTUN_CONTROL_NAME: &str = "com.apple.net.utun_control";
const SYSPROTO_CONTROL: c_int = 2;
const AF_SYSTEM: c_int = 32;
const PF_SYSTEM: c_int = AF_SYSTEM;
const SOCK_DGRAM: c_int = 2;

// Define missing ioctl constants for macOS
const SIOCSIFADDR: u64 = 0x8020690c;
const SIOCSIFDSTADDR: u64 = 0x8020690e;
const SIOCSIFFLAGS: u64 = 0x80206910;
const SIOCGIFFLAGS: u64 = 0xc0206911;
const SIOCSIFMTU: u64 = 0x80206934;
const SIOCGIFMTU: u64 = 0xc0206935;
const SIOCSIFNETMASK: u64 = 0x80206916;
const SIOCGIFNETMASK: u64 = 0xc0206925;
const SIOCGIFADDR: u64 = 0xc0206921;
const SIOCGIFDSTADDR: u64 = 0xc0206922;

// Define the control info struct
#[repr(C)]
struct CtlInfo {
    ctl_id: u32,
    ctl_name: [c_char; 96],
}

// Define the sockaddr_ctl struct for connecting to the utun control device
#[repr(C)]
struct SockaddrCtl {
    sc_len: u8,
    sc_family: u8,
    ss_sysaddr: u16,
    sc_id: u32,
    sc_unit: u32,
    sc_reserved: [u32; 5],
}

#[derive(Clone)]
pub struct Interface {
    fds: Vec<i32>,
    socket: i32,
    name: String,
}

impl Interface {
    pub fn new(fds: Vec<i32>, name: &str, _flags: i16) -> Result<Self> {
        Ok(Interface {
            fds,
            socket: unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0) },
            name: name.to_owned(),
        })
    }

    pub fn init(&self, params: Params) -> Result<()> {
        if let Some(mtu) = params.mtu {
            self.mtu(Some(mtu))?;
        }
        if let Some(address) = params.address {
            self.address(Some(address))?;
        }
        if let Some(netmask) = params.netmask {
            self.netmask(Some(netmask))?;
        }
        if let Some(destination) = params.destination {
            self.destination(Some(destination))?;
        }
        if let Some(broadcast) = params.broadcast {
            self.broadcast(Some(broadcast))?;
        }
        if params.up {
            self.flags(Some(libc::IFF_UP as i16 | libc::IFF_RUNNING as i16))?;
        }

        // Handle owner and group for macOS
        if let Some(owner) = params.owner {
            // On macOS, we would typically use chown for the device
            // but it's not directly available like in Linux
            // Log this information instead
            eprintln!("Note: Setting owner to {} is not supported on macOS", owner);
        }

        if let Some(group) = params.group {
            // Similarly for group
            eprintln!("Note: Setting group to {} is not supported on macOS", group);
        }

        // Handle persistence
        if params.persist {
            // macOS utun devices are persistent by default but log this info
            eprintln!("Note: macOS utun devices are already persistent by default");
        }

        Ok(())
    }

    pub fn files(&self) -> &[i32] {
        &self.fds
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn mtu(&self, mtu: Option<i32>) -> Result<i32> {
        let mut req = ifreq::new(self.name());
        if let Some(mtu) = mtu {
            req.ifr_ifru.ifru_mtu = mtu;
            unsafe {
                if libc::ioctl(self.socket, SIOCSIFMTU, &req) < 0 {
                    return Err(std::io::Error::last_os_error().into());
                }
            }
        } else {
            unsafe {
                if libc::ioctl(self.socket, SIOCGIFMTU, &mut req) < 0 {
                    return Err(std::io::Error::last_os_error().into());
                }
            }
        }
        Ok(unsafe { req.ifr_ifru.ifru_mtu })
    }

    pub fn netmask(&self, netmask: Option<Ipv4Addr>) -> Result<Ipv4Addr> {
        let mut req = ifreq::new(self.name());
        if let Some(netmask) = netmask {
            req.ifr_ifru.ifru_netmask = netmask.to_address();
            unsafe {
                if libc::ioctl(self.socket, SIOCSIFNETMASK, &req) < 0 {
                    return Err(std::io::Error::last_os_error().into());
                }
            }
            return Ok(netmask);
        }
        unsafe {
            if libc::ioctl(self.socket, SIOCGIFNETMASK, &mut req) < 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        Ok(unsafe { Ipv4Addr::from_address(req.ifr_ifru.ifru_netmask) })
    }

    pub fn address(&self, address: Option<Ipv4Addr>) -> Result<Ipv4Addr> {
        let mut req = ifreq::new(self.name());
        if let Some(address) = address {
            req.ifr_ifru.ifru_addr = address.to_address();
            unsafe {
                if libc::ioctl(self.socket, SIOCSIFADDR, &req) < 0 {
                    return Err(std::io::Error::last_os_error().into());
                }
            }
            return Ok(address);
        }
        unsafe {
            if libc::ioctl(self.socket, SIOCGIFADDR, &mut req) < 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        Ok(unsafe { Ipv4Addr::from_address(req.ifr_ifru.ifru_addr) })
    }

    pub fn destination(&self, dst: Option<Ipv4Addr>) -> Result<Ipv4Addr> {
        let mut req = ifreq::new(self.name());
        if let Some(dst) = dst {
            req.ifr_ifru.ifru_dstaddr = dst.to_address();
            unsafe {
                if libc::ioctl(self.socket, SIOCSIFDSTADDR, &req) < 0 {
                    return Err(std::io::Error::last_os_error().into());
                }
            }
            return Ok(dst);
        }
        unsafe {
            if libc::ioctl(self.socket, SIOCGIFDSTADDR, &mut req) < 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        Ok(unsafe { Ipv4Addr::from_address(req.ifr_ifru.ifru_dstaddr) })
    }

    pub fn broadcast(&self, broadcast: Option<Ipv4Addr>) -> Result<Ipv4Addr> {
        // On macOS with utun interfaces, broadcast is not applicable as these are point-to-point
        // interfaces. We should just return the broadcast that was set or one derived from
        // the IP address and netmask
        if let Some(broadcast) = broadcast {
            // Just pretend we set it successfully
            return Ok(broadcast);
        }

        // If no broadcast is specified, we could try to compute one from the
        // IP address and netmask or just return a default
        match (self.address(None), self.netmask(None)) {
            (Ok(addr), Ok(mask)) => {
                let addr_bits = u32::from_be_bytes(addr.octets());
                let mask_bits = u32::from_be_bytes(mask.octets());
                let broadcast_bits = addr_bits | !mask_bits;
                Ok(Ipv4Addr::from(broadcast_bits.to_be_bytes()))
            }
            // If we can't get the address or netmask, return a reasonable default
            _ => Ok(Ipv4Addr::new(255, 255, 255, 255)),
        }
    }

    pub fn flags(&self, flags: Option<i16>) -> Result<i16> {
        let mut req = ifreq::new(self.name());
        unsafe {
            if libc::ioctl(self.socket, SIOCGIFFLAGS, &mut req) < 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        if let Some(flags) = flags {
            unsafe { req.ifr_ifru.ifru_flags |= flags };
            unsafe {
                if libc::ioctl(self.socket, SIOCSIFFLAGS, &req) < 0 {
                    return Err(std::io::Error::last_os_error().into());
                }
            }
        }
        Ok(unsafe { req.ifr_ifru.ifru_flags })
    }

    // Create a new utun device
    pub fn open_utun(unit: i32) -> Result<(i32, String)> {
        let fd = unsafe { libc::socket(PF_SYSTEM, SOCK_DGRAM, SYSPROTO_CONTROL) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }

        let control_name = CString::new(UTUN_CONTROL_NAME).unwrap();
        let mut info: CtlInfo = unsafe { mem::zeroed() };

        unsafe {
            ptr::copy_nonoverlapping(
                control_name.as_ptr(),
                info.ctl_name.as_mut_ptr(),
                control_name.as_bytes().len(),
            );
        }

        if unsafe { libc::ioctl(fd, CTLIOCGINFO, &mut info as *mut _ as *mut c_void) } < 0 {
            unsafe { libc::close(fd) };
            return Err(std::io::Error::last_os_error().into());
        }

        let mut addr: SockaddrCtl = unsafe { mem::zeroed() };
        addr.sc_id = info.ctl_id;
        addr.sc_len = mem::size_of::<SockaddrCtl>() as u8;
        addr.sc_family = AF_SYSTEM as u8;
        addr.ss_sysaddr = 2; // AF_SYS_CONTROL
        addr.sc_unit = unit as u32;

        if unsafe {
            libc::connect(
                fd,
                &addr as *const _ as *const libc::sockaddr,
                mem::size_of::<SockaddrCtl>() as libc::socklen_t,
            )
        } < 0
        {
            unsafe { libc::close(fd) };
            return Err(std::io::Error::last_os_error().into());
        }

        // Get the interface name
        let mut name_buf = [0u8; 64];
        let mut name_len: libc::socklen_t = name_buf.len() as libc::socklen_t;
        let name_ptr = name_buf.as_mut_ptr() as *mut c_void;

        if unsafe {
            libc::getsockopt(
                fd,
                SYSPROTO_CONTROL,
                2, // UTUN_OPT_IFNAME
                name_ptr,
                &mut name_len,
            )
        } < 0
        {
            unsafe { libc::close(fd) };
            return Err(std::io::Error::last_os_error().into());
        }

        // Extract the interface name (null-terminated C string)
        let name = String::from_utf8_lossy(&name_buf[..name_len as usize - 1]).to_string();
        Ok((fd, name))
    }
}

impl Drop for Interface {
    fn drop(&mut self) {
        unsafe { libc::close(self.socket) };
    }
}
