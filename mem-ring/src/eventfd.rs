use std::{
    future::poll_fn,
    io::Error,
    os::fd::{AsRawFd, IntoRawFd, RawFd},
};

#[cfg(feature = "monoio")]
use monoio::io::AsyncFd;

#[cfg(all(feature = "tokio", not(feature = "monoio")))]
use tokio::io::unix::AsyncFd;

pub(crate) struct Notifier {
    // for linux, it is eventfd
    fd: RawFd,
    do_drop: bool,
}

pub(crate) struct Awaiter {
    // for linux, it is eventfd
    fd: RawFd,
    afd: AsyncFd<RawFd>,
    do_drop: bool,
}

impl AsRawFd for Notifier {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl AsRawFd for Awaiter {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl Notifier {
    pub(crate) fn new() -> Result<Self, Error> {
        let fd = unsafe { libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC) };
        if fd == -1 {
            return Err(Error::last_os_error());
        }
        Ok(Self { fd, do_drop: true })
    }

    #[inline]
    pub(crate) unsafe fn from_raw_fd(fd: RawFd) -> Self {
        Self { fd, do_drop: true }
    }

    #[inline]
    pub(crate) fn mark_drop(&mut self, drop: bool) {
        self.do_drop = drop;
    }

    pub(crate) fn notify(&self) -> Result<(), Error> {
        loop {
            unsafe {
                let buf: u64 = 1;
                if libc::write(
                    self.fd,
                    &buf as *const u64 as *const libc::c_void,
                    std::mem::size_of::<u64>(),
                ) != -1
                {
                    return Ok(());
                }
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
    pub(crate) fn new() -> Result<Self, Error> {
        let fd = unsafe { libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC) };
        if fd == -1 {
            return Err(Error::last_os_error());
        }
        let afd = AsyncFd::new(fd)?;
        Ok(Self {
            fd,
            afd,
            do_drop: true,
        })
    }

    pub(crate) unsafe fn from_raw_fd(fd: RawFd) -> Result<Self, Error> {
        let afd = AsyncFd::new(fd)?;
        Ok(Self {
            fd,
            afd,
            do_drop: true,
        })
    }

    pub(crate) fn mark_drop(&mut self, drop: bool) {
        self.do_drop = drop;
    }

    pub(crate) async fn wait(&mut self) {
        let mut guard = poll_fn(|cx| self.afd.poll_read_ready(cx)).await.unwrap();
        unsafe {
            libc::read(
                self.fd,
                &mut 0u64 as *mut u64 as *mut libc::c_void,
                std::mem::size_of::<u64>(),
            )
        };
        guard.clear_ready();
    }
}

impl Drop for Notifier {
    fn drop(&mut self) {
        if self.do_drop {
            unsafe {
                libc::close(self.fd);
            }
        }
    }
}

impl Drop for Awaiter {
    fn drop(&mut self) {
        if self.do_drop {
            unsafe {
                libc::close(self.fd);
            }
        }
    }
}

impl IntoRawFd for Notifier {
    fn into_raw_fd(mut self) -> RawFd {
        self.do_drop = false;
        self.fd
    }
}

impl IntoRawFd for Awaiter {
    fn into_raw_fd(mut self) -> RawFd {
        self.do_drop = false;
        self.fd
    }
}
