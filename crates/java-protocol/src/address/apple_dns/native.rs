//! The DNS service owns its socket; Tokio only registers readiness for it.
use super::{INTERNET_CLASS, Reply, SRV, SRV_TYPE, dns_error};
use std::{
    ffi::{CString, c_char, c_void},
    io,
    os::fd::{AsRawFd, RawFd},
    ptr,
    time::Duration,
};
use tokio::io::unix::AsyncFd;

type ServiceRef = *mut c_void;
type Callback = unsafe extern "C" fn(
    ServiceRef,
    u32,
    u32,
    i32,
    *const c_char,
    u16,
    u16,
    u16,
    *const c_void,
    u32,
    *mut c_void,
);

#[link(name = "dns_sd")]
unsafe extern "C" {
    fn DNSServiceQueryRecord(
        service: *mut ServiceRef,
        flags: u32,
        interface: u32,
        name: *const c_char,
        kind: u16,
        class: u16,
        callback: Callback,
        context: *mut c_void,
    ) -> i32;
    fn DNSServiceRefSockFD(service: ServiceRef) -> i32;
    fn DNSServiceProcessResult(service: ServiceRef) -> i32;
    fn DNSServiceRefDeallocate(service: ServiceRef);
}

struct Query {
    service: ServiceRef,
    reply: Box<Reply>,
}

// DNS-SD permits serialized use across threads. This unique owner is not Sync;
// callbacks run synchronously inside process(), never on a separate dispatch queue.
unsafe impl Send for Query {}

impl Drop for Query {
    fn drop(&mut self) {
        // Cancels callbacks and closes the service socket before the context is freed.
        unsafe { DNSServiceRefDeallocate(self.service) };
    }
}

impl Query {
    fn new(name: &str) -> io::Result<Self> {
        let name = CString::new(name).map_err(|_| io::Error::other("Invalid DNS name"))?;
        let mut reply = Box::<Reply>::default();
        let mut service = ptr::null_mut();
        // ReturnIntermediates makes authoritative NXDOMAIN observable. Without it,
        // absent SRV records could wait forever. We ignore the intermediate CNAMEs.
        let error = unsafe {
            DNSServiceQueryRecord(
                &mut service,
                0x1000,
                0,
                name.as_ptr(),
                SRV_TYPE,
                INTERNET_CLASS,
                callback,
                (&raw mut *reply).cast(),
            )
        };
        if error != 0 {
            // Per the API contract no handle is initialized on failure.
            return Err(dns_error(error));
        }
        Ok(Self { service, reply })
    }

    fn process(&mut self) -> io::Result<()> {
        // Only invoked after readiness; no other code reads this dedicated socket.
        let code = unsafe { DNSServiceProcessResult(self.service) };
        if code == 0 {
            Ok(())
        } else {
            Err(dns_error(code))
        }
    }
}

/// Tokio readiness can be spurious; never call the blocking DNS-SD reader then.
fn has_data(fd: RawFd) -> io::Result<bool> {
    let mut poll = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let result = unsafe { libc::poll(&mut poll, 1, 0) };
    if result < 0 {
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::Interrupted {
            return Ok(false);
        }
        return Err(error);
    }
    Ok(result != 0)
}

unsafe extern "C" fn callback(
    _service: ServiceRef,
    flags: u32,
    _interface: u32,
    error: i32,
    _name: *const c_char,
    kind: u16,
    class: u16,
    len: u16,
    data: *const c_void,
    _ttl: u32,
    context: *mut c_void,
) {
    // Context outlives the service. All access is serialized through process().
    let reply = unsafe { &mut *context.cast::<Reply>() };
    // Other fields are undefined on errors. Do not dereference them.
    let data = if error != 0 || len == 0 || data.is_null() {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(data.cast::<u8>(), usize::from(len)) }
    };
    reply.accept(flags, error, kind, class, data);
}

struct Socket(RawFd);
impl AsRawFd for Socket {
    fn as_raw_fd(&self) -> RawFd {
        self.0
    }
}

/// Dropping or timing out the future deregisters readiness and cancels the query.
pub(crate) async fn lookup(name: &str) -> io::Result<Vec<SRV>> {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut query = Query::new(name)?;
        let fd = unsafe { DNSServiceRefSockFD(query.service) };
        if fd < 0 {
            return Err(io::Error::other("Apple DNS service has no socket"));
        }
        // Declared after query: deregistration happens before the service closes fd.
        let socket = AsyncFd::new(Socket(fd))?;
        loop {
            let mut ready = socket.readable().await?;
            if has_data(fd)? {
                query.process()?;
            }
            ready.clear_ready();
            if query.reply.done {
                return match query.reply.error.take() {
                    Some(error) => Err(error),
                    None => Ok(std::mem::take(&mut query.reply.records)),
                };
            }
        }
    })
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Apple DNS SRV query timed out"))?
}
