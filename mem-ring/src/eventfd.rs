// Copyright 2024 ihciah. All Rights Reserved.

use std::{
    io::Error,
    os::fd::{AsRawFd, FromRawFd, RawFd},
};

#[cfg(feature = "monoio")]
use monoio::{io::AsyncReadRent, net::UnixStream};

#[cfg(all(feature = "tokio", not(feature = "monoio")))]
use tokio::{io::AsyncReadExt, net::UnixStream};

pub(crate) fn new_pair() -> Result<(RawFd, RawFd), Error> {
    // create unix stream pair
    let mut fds = [-1; 2];
    #[cfg(any(
        target_os = "android",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "illumos",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "linux"
    ))]
    let flag = libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC;
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    let flag = libc::SOCK_STREAM;

    macro_rules! ret_last_error {
        ($fn: ident ( $($arg: expr),* $(,)* )) => {
            if ::libc::$fn($($arg, )*) == -1 {
                return Err(Error::last_os_error());
            }
        };
        ($fn: ident ( $($arg: expr),* $(,)* ), $fds: ident) => {
            if ::libc::$fn($($arg, )*) == -1 {
                for fd in $fds {
                    ::libc::close(fd);
                }
                return Err(Error::last_os_error());
            }
        };
    }

    unsafe {
        ret_last_error!(socketpair(libc::AF_UNIX, flag, 0, fds.as_mut_ptr()));
        #[cfg(target_vendor = "apple")]
        for fd in fds.iter() {
            ret_last_error!(
                setsockopt(
                    *fd,
                    libc::SOL_SOCKET,
                    libc::SO_NOSIGPIPE,
                    &1 as *const _ as *const libc::c_void,
                    std::mem::size_of::<libc::c_int>() as u32,
                ),
                fds
            );
        }

        #[cfg(any(target_os = "ios", target_os = "macos"))]
        for fd in fds.iter() {
            ret_last_error!(fcntl(*fd, libc::F_SETFL, libc::O_NONBLOCK), fds);
        }
    }

    Ok((fds[0], fds[1]))
}

pub(crate) struct Notifier {
    fd: RawFd,
}

pub(crate) struct Awaiter {
    unix_stream: UnixStream,
}

impl AsRawFd for Notifier {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl AsRawFd for Awaiter {
    fn as_raw_fd(&self) -> RawFd {
        self.unix_stream.as_raw_fd()
    }
}

impl Notifier {
    #[allow(unused)]
    pub(crate) fn new() -> Result<(Self, RawFd), Error> {
        let (fd, peer) = new_pair()?;
        Ok((Self { fd }, peer))
    }

    #[inline]
    pub(crate) unsafe fn from_raw_fd(fd: RawFd) -> Self {
        Self { fd }
    }

    pub(crate) fn notify(&self) -> Result<(), Error> {
        const DATA: u8 = 0;
        loop {
            if unsafe { libc::write(self.fd, &DATA as *const u8 as *const libc::c_void, 1) } != -1 {
                return Ok(());
            }

            let err = Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(err);
        }
    }
}

impl Awaiter {
    #[allow(unused)]
    pub(crate) fn new() -> Result<(Self, RawFd), Error> {
        let (fd, peer) = new_pair()?;
        Ok((unsafe { Self::from_raw_fd(fd) }?, peer))
    }

    pub(crate) unsafe fn from_raw_fd(fd: RawFd) -> Result<Self, Error> {
        let std_unix_stream = unsafe { std::os::unix::net::UnixStream::from_raw_fd(fd) };
        let unix_stream = UnixStream::from_std(std_unix_stream)?;
        Ok(Self { unix_stream })
    }

    #[cfg(feature = "monoio")]
    pub(crate) async fn wait(&mut self) {
        // Pass an owned buffer to the read op so the buffer lives as long as
        // the op itself. Using a thread_local buffer with a raw pointer is
        // unsound: the spawned task holding this read may leak (outlive the
        // runtime) and the kernel could write into freed TLS memory.
        let buf = vec![0; 64];
        let _ = self.unix_stream.read(buf).await;
    }

    #[cfg(all(feature = "tokio", not(feature = "monoio")))]
    pub(crate) async fn wait(&mut self) {
        let mut buf: [u8; 64] = [0; 64];
        let _ = self.unix_stream.read(&mut buf).await;
    }
}

impl Drop for Notifier {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_pair_and_notify() {
        let (notifier, peer) = Notifier::new().unwrap();
        assert!(notifier.as_raw_fd() >= 0);
        assert!(peer >= 0);

        notifier.notify().unwrap();
        notifier.notify().unwrap();

        let mut buf = [0u8; 8];
        let n = unsafe { libc::read(peer, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
        assert_eq!(n, 2);

        unsafe { libc::close(peer) };
    }
}
