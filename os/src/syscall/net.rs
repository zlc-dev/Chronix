use core::{any::Any, panic};

use alloc::{sync::Arc, vec::Vec};
use fatfs::info;
use hal::{addr, println};
use lwext4_rust::bindings::EXT4_SUPERBLOCK_FLAGS_TEST_FILESYS;

use crate::{fs::OpenFlags, net::{addr::{SockAddr, SockAddrIn4, SockAddrIn6}, socket::{self, Sock}, SaFamily}, signal::SigSet, task::{current_task, fs::FdInfo}, utils::yield_now};

use super::{SysError, SysResult};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
/// Socket types
pub enum SocketType {
    /// TCP
    STREAM = 1,
    /// UDP
    DGRAM = 2,
    /// Raw IP
    RAW = 3,
    /// RDM
    RDM = 4,
    /// Seq Packet
    SEQPACKET = 5,
    /// DCCP
    DCCP = 6,
    /// Packet
    PACKET = 10,
}

impl TryFrom<i32> for SocketType {
    type Error = SysError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::STREAM),
            2 => Ok(Self::DGRAM),
            3 => Ok(Self::RAW),
            4 => Ok(Self::RDM),
            5 => Ok(Self::SEQPACKET),
            6 => Ok(Self::DCCP),
            10 => Ok(Self::PACKET),
            _ => Err(Self::Error::EINVAL),
        }
    }
}

/// Set O_NONBLOCK flag on the open fd
pub const SOCK_NONBLOCK: i32 = 0x800;
/// Set FD_CLOEXEC flag on the new fd
pub const SOCK_CLOEXEC: i32 = 0x80000;

/// create an endpoint for communication and returns a file decriptor refers to the endpoint
/// Since Linux 2.6.27, the type argument serves a second purpose: in
///addition to specifying a socket type, it may include the bitwise
///OR of any of the following values, to modify the behavior of
///socket():
// SOCK_NONBLOCK
//        Set the O_NONBLOCK file status flag on the open file
//        description (see open(2)) referred to by the new file
//        descriptor.  Using this flag saves extra calls to fcntl(2)
//        to achieve the same result.

// SOCK_CLOEXEC
//        Set the close-on-exec (FD_CLOEXEC) flag on the new file
//        descriptor.  See the description of the O_CLOEXEC flag in
//        open(2) for reasons why this may be useful.
pub fn sys_socket(domain: usize, types: usize, _protocol: usize) -> SysResult {
    let domain = SaFamily::try_from(domain as u16)?;
    let mut types = types as i32;
    let mut nonblock = false;
    // file descriptor flags
    let mut flags = OpenFlags::empty();
    if types & SOCK_NONBLOCK != 0 {
        nonblock = true;
        types &= !SOCK_NONBLOCK;
        flags |= OpenFlags::O_NONBLOCK;
    } 
    if types & SOCK_CLOEXEC != 0 {
        types &= !SOCK_CLOEXEC;
        flags |= OpenFlags::O_CLOEXEC;
    }

    let types = SocketType::try_from(types)?;
    let socket = socket::Socket::new(domain,types, nonblock);
    let fd_info = FdInfo {
        file: Arc::new(socket),
        flags: flags.into(),
    };
    let task = current_task().unwrap();
    let fd = task.with_mut_fd_table(|t|t.alloc_fd());
    task.with_mut_fd_table(|t| {
        t.put_file(fd, fd_info).or_else(|e|Err(e))
    })?;
    // log::info!("sys_socket fd: {}", fd);
    Ok(fd as isize)
}
/// “assigning a name to a socket”
pub fn sys_bind(fd: usize, addr: usize, addr_len: usize) -> SysResult {
    let task = current_task().unwrap();
    let family = SaFamily::try_from(unsafe {
        *(addr as *const u16)
    })?;
    let local_addr = match family {
        SaFamily::AfInet => {
            if addr_len < size_of::<SockAddrIn4>() {
                return Err(SysError::EINVAL);
            }
            Ok(SockAddr{
                ipv4: unsafe { *(addr as *const _)},
            })
        }
        SaFamily::AfInet6 => {
            if addr_len < size_of::<SockAddrIn6>() {
                return Err(SysError::EINVAL);
            }
            Ok(SockAddr{
                ipv6: unsafe {
                    *(addr as *const _)
                }
            })
        },
    }?;
    // log::info!("[sys_bind] local_addr's port is: {}",unsafe {
        // local_addr.ipv4.sin_port
    // });
    let socket_file = task.with_fd_table(|table| {
        table.get_file(fd)})?
        .downcast_arc::<socket::Socket>().unwrap_or_else(|_| {
        panic!("Failed to downcast to socket::Socket")
    });
    socket_file.sk.bind(fd, local_addr)?;
    Ok(0)
}

/// Mark the stream socket referenced by the file descriptor `sockfd` as
/// passive. This socket will be used later to accept connections from other
/// (active) sockets
pub fn sys_listen(fd: usize, _backlog: usize) -> SysResult {
    let current_task = current_task().unwrap();
    let socket_file = current_task.with_fd_table(|table| {
        table.get_file(fd)})?
        .downcast_arc::<socket::Socket>()
        .unwrap_or_else(|_| {
            panic!("Failed to downcast to socket::Socket")
        });
    socket_file.sk.listen()?;
    Ok(0)
}

/// Connect the active socket refrenced by the file descriptor `sockfd` to the
/// address specified by `addr`. The `addr` argument is a pointer to a
/// `sockaddr` structure that contains the address of the remote socket.
/// The `addrlen` argument specifies the size of this structure.
pub async fn sys_connect(fd: usize, addr: usize, addr_len: usize) -> SysResult {
    let task = current_task().unwrap();
    let remote_addr = match SaFamily::try_from(unsafe {
        *(addr as *const u16)
    })? {
        SaFamily::AfInet => {
            if addr_len < size_of::<SockAddrIn4>() {
                return Err(SysError::EINVAL);
            }
            Ok(SockAddr{
                ipv4: unsafe { *(addr as *const _) },
            })
        }
        SaFamily::AfInet6 => {
            if addr_len < size_of::<SockAddrIn6>() {
                return Err(SysError::EINVAL);
            }
            Ok(SockAddr{
                ipv6: unsafe { *(addr as *const _) },
            })
        }
    }?;
    // log::info!("[sys_connect] remote_addr's port is: {}",
        // unsafe {
            // remote_addr.ipv4.sin_port
    // });
    let socket_file = task.with_fd_table(|table| {
        table.get_file(fd)})?
        .downcast_arc::<socket::Socket>()
        .unwrap_or_else(|_| {
            panic!("Failed to downcast to socket::Socket")
        });
    socket_file.sk.connect(remote_addr.into_endpoint()).await?;
    Ok(0)
}

/// Accept a connection on the socket `sockfd` that is ready to be accepted.
/// The `addr` argument is a pointer to a `sockaddr` structure that will
/// hold the address of the peer socket, and `addrlen` is a pointer to
/// an integer that will hold the size of this structure.
///
/// The `sockfd` argument is a socket that has been created with the
/// `SOCK_STREAM` type, has been bound to a local address with `bind`,
/// and is listening for connections after a `listen` system call.
///
/// The `accept` system call is used on a socket that is listening for
/// incoming connections. It extracts the first connection request on
/// the queue of pending connections, creates a new socket for the
/// connection, and returns a new file descriptor referring to that
/// socket. The newly created socket is usually in the `ESTABLISHED`

pub async fn sys_accept(fd: usize, addr: usize, addr_len: usize) -> SysResult {
    let task = current_task().unwrap();
    let socket_file = task.with_fd_table(|table| {
        table.get_file(fd)})?
        .downcast_arc::<socket::Socket>()
        .unwrap_or_else(|_| {
            panic!("Failed to downcast to socket::Socket")
        });
    // moniter accept, allow sig_kill and sig_stop to interrupt
    task.set_interruptable();
    // task.set_wake_up_sigs(SigSet::SIGKILL | SigSet::SIGSTOP);
    let accept_sk = socket_file.sk.accept().await?;
    task.set_running();
    log::info!("get accept correct");
    let peer_addr_endpoint = accept_sk.peer_addr().unwrap();
    let peer_addr = SockAddr::from_endpoint(peer_addr_endpoint);
    // log::info!("Accept a connection from {:?}", peer_addr);
    // write to pointer
    unsafe {
        match SaFamily::try_from(peer_addr.family).unwrap() {
            SaFamily::AfInet => {
                let addr_ptr = addr as *mut SockAddrIn4;
                addr_ptr.write_volatile(peer_addr.ipv4);
                let addr_len_ptr = addr_len as *mut u32;
                addr_len_ptr.write_volatile(size_of::<SockAddrIn4>() as u32);
            }
            SaFamily::AfInet6 => {
                let addr_ptr = addr as *mut SockAddrIn6;
                addr_ptr.write_volatile(peer_addr.ipv6);
                let addr_len_ptr = addr_len as *mut u32;
                addr_len_ptr.write_volatile(size_of::<SockAddrIn6>() as u32);
            },
        }
    }

    let accept_socket = Arc::new(socket::Socket::from_another(&socket_file, Sock::TCP(accept_sk)));
    let fd_info = FdInfo {
        file: accept_socket,
        flags: OpenFlags::empty().into(),
    };
    let new_fd = task.with_mut_fd_table(|t|t.alloc_fd());
    task.with_mut_fd_table(|t| {
        t.put_file(new_fd, fd_info)
    })?;
    Ok(new_fd as isize)
}

/// The system calls send(), sendto(), and sendmsg() are used to
/// transmit a message to another socket.
pub async fn sys_sendto(
    fd: usize,
    buf: usize,
    len: usize,
    _flags: usize,
    addr: usize,
    addr_len: usize,
)-> SysResult {
    // log::info!("addr is {}, addr_len is {}", addr, addr_len);
    let buf_slice = buf as *const u8 ;
    let task = current_task().unwrap();
    let buf_slice = unsafe {
        core::slice::from_raw_parts_mut(buf_slice as *mut u8, len)
    };
    let socket_file = task.with_fd_table(|table| {
        table.get_file(fd)})?
        .downcast_arc::<socket::Socket>()
        .unwrap_or_else(|_| {
            panic!("Failed to downcast to socket::Socket")
        });
    task.set_interruptable();
    let bytes = match socket_file.sk_type {
        SocketType::DGRAM => {
            let remote_addr = if addr != 0 {  Some(
                match SaFamily::try_from(unsafe {
                    *(addr as *const u16)
                })? {
                    SaFamily::AfInet => {
                        if addr_len < size_of::<SockAddrIn4>() {
                            log::warn!("sys_sendto: addr_len < size_of::<SockAddrIn4>() which is {}",size_of::<SockAddrIn4>());
                            return Err(SysError::EINVAL);
                        }
                        Ok(SockAddr{
                            ipv4: unsafe { *(addr as *const _) },
                        })
                    }
                    SaFamily::AfInet6 => {
                        if addr_len < size_of::<SockAddrIn6>() {
                            return Err(SysError::EINVAL);
                        }
                        Ok(SockAddr{
                            ipv6: unsafe { *(addr as *const _) },
                        })
                    }
                }?
            .into_endpoint())}else {
                None
            };
            socket_file.sk.send(&buf_slice, remote_addr).await?    
        }
        SocketType::STREAM => {
            if addr != 0 {
                return Err(SysError::EISCONN);
            }
            socket_file.sk.send(&buf_slice, None).await?
        },
        _ => todo!(),
    };
    task.set_running();
    Ok(bytes as isize)
}

/// The recvfrom() function shall receive a message from a connection-
/// mode or connectionless-mode socket. It is normally used with
/// connectionless-mode sockets because it permits the application to
/// retrieve the source address of received data.
pub async fn sys_recvfrom(
    sockfd: usize,
    buf: usize,
    len: usize,
    _flags: usize,
    addr: usize,
    addrlen: usize,
) -> SysResult {
    // log::info!("[sys_recvfrom] sockfd: {}, buf: {:#x}, len: {}, flags: {}, addr: {:#x}, addrlen: {}", sockfd, buf, len, _flags, addr, addrlen);
    let task = current_task().unwrap();
    let socket_file = task.with_fd_table(|table| {
        table.get_file(sockfd)})?
        .downcast_arc::<socket::Socket>()
        .unwrap_or_else(|_| {
            panic!("Failed to downcast to socket::Socket")
        });
    let mut inner_vec = Vec::with_capacity(len);
    unsafe {
        inner_vec.set_len(len);
    }
    task.set_interruptable();
    let (bytes, remote_endpoint) = socket_file.sk.recv(&mut inner_vec).await?;
    // log::info!("recvfrom: bytes: {}, remote_endpoint: {:?}", bytes, remote_endpoint);
    let remote_addr = SockAddr::from_endpoint(remote_endpoint);
    task.set_running();
    // write to pointer
    let buf_slice = unsafe {
        core::slice::from_raw_parts_mut(buf as *mut u8, bytes)
    };
    buf_slice[..bytes].copy_from_slice(&inner_vec[..bytes]);
    // write to sockaddr_in
    unsafe {
        match SaFamily::try_from(remote_addr.family).unwrap() {
            SaFamily::AfInet => {
                let addr_ptr = addr as *mut SockAddrIn4;
                addr_ptr.write_volatile(remote_addr.ipv4);
                let addr_len_ptr = addrlen as *mut u32;
                addr_len_ptr.write_volatile(size_of::<SockAddrIn4>() as u32);
            }
            SaFamily::AfInet6 => {
                let addr_ptr = addr as *mut SockAddrIn6;
                addr_ptr.write_volatile(remote_addr.ipv6);
                let addr_len_ptr = addrlen as *mut u32;
                addr_len_ptr.write_volatile(size_of::<SockAddrIn6>() as u32);
            },
        }
    }
    // log::info!("now return bytes: {}",bytes);
    Ok(bytes as isize)
}
/// Returns the local address of the Socket corresponding to `sockfd`.
pub fn sys_getsockname(fd: usize, addr: usize, addr_len: usize) -> SysResult {
    let task = current_task().unwrap();
    let socket_file = task.with_fd_table(|table| {
        table.get_file(fd)
        .clone()
        .unwrap()
        .downcast_arc::<socket::Socket>()
        .unwrap_or_else(|_| {
            panic!("Failed to downcast to socket::Socket")
        })
    });
    let local_addr = socket_file.sk.local_addr().unwrap();
    log::info!("Get local address of socket: {:?}", local_addr);
    // write to pointer
    unsafe {
        match SaFamily::try_from(local_addr.family).unwrap() {
            SaFamily::AfInet => {
                let addr_ptr = addr as *mut SockAddrIn4;
                addr_ptr.write_volatile(local_addr.ipv4);
                let addr_len_ptr = addr_len as *mut u32;
                addr_len_ptr.write_volatile(size_of::<SockAddrIn4>() as u32);
            }
            SaFamily::AfInet6 => {
                let addr_ptr = addr as *mut SockAddrIn6;
                addr_ptr.write_volatile(local_addr.ipv6);
                let addr_len_ptr = addr_len as *mut u32;
                addr_len_ptr.write_volatile(size_of::<SockAddrIn6>() as u32);
            },
        }
    }
    Ok(0)
}

/// returns the peer address of the socket corresponding to the file descriptor `sockfd`
pub fn sys_getpeername(fd: usize, addr: usize, addr_len: usize) -> SysResult {
    let task = current_task().unwrap();
    let socket_file = task.with_fd_table(|table| {
        table.get_file(fd)})?
        .downcast_arc::<socket::Socket>()
        .unwrap_or_else(|_| {
            panic!("Failed to downcast to socket::Socket")
        });
    let peer_addr = socket_file.sk.peer_addr().unwrap();
    log::info!("Get peer address of socket: {:?}", peer_addr);
    // write to pointer
    unsafe {
        match SaFamily::try_from(peer_addr.family).unwrap() {
            SaFamily::AfInet => {
                let addr_ptr = addr as *mut SockAddrIn4;
                addr_ptr.write_volatile(peer_addr.ipv4);
                let addr_len_ptr = addr_len as *mut u32;
                addr_len_ptr.write_volatile(size_of::<SockAddrIn4>() as u32);
            }
            SaFamily::AfInet6 => {
                let addr_ptr = addr as *mut SockAddrIn6;
                addr_ptr.write_volatile(peer_addr.ipv6);
                let addr_len_ptr = addr_len as *mut u32;
                addr_len_ptr.write_volatile(size_of::<SockAddrIn6>() as u32);
            },
        }
    }
    Ok(0)
}