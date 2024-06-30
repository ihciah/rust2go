use std::{
    cell::UnsafeCell,
    io::Error,
    os::fd::{AsRawFd, FromRawFd, RawFd},
};

#[cfg(feature = "monoio")]
use monoio::{buf::RawBuf, io::AsyncReadRent, net::UnixStream};

#[cfg(all(feature = "tokio", not(feature = "monoio")))]
use tokio::{io::AsyncReadExt, net::UnixStream};

pub(crate) fn new_pair() -> Result<(RawFd, RawFd), Error> {
    // create unix stream pair
    let mut fds = [-1; 2];
    if unsafe {
        libc::socketpair(
            libc::AF_UNIX,
            libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
            0,
            fds.as_mut_ptr(),
        )
    } == -1
    {
        return Err(Error::last_os_error());
    }

    Ok((fds[0], fds[1]))
}

#[allow(unused)]
pub(crate) fn dup(fd: RawFd) -> Result<RawFd, Error> {
    let fd = unsafe { libc::dup(fd) };
    if fd == -1 {
        return Err(Error::last_os_error());
    }
    Ok(fd)
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
        thread_local! {
            pub static BUF: UnsafeCell<Vec<u8>> = UnsafeCell::new(vec![0; 64]);
        }
        let buf = BUF.with(|buf| {
            let buf = unsafe { &(*buf.get()) };
            unsafe { RawBuf::new(buf.as_ptr(), buf.len()) }
        });
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
