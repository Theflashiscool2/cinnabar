//! One-shot SRV queries through Apple's network-aware DNS service.
//!
//! API contract: <https://github.com/apple-oss-distributions/mDNSResponder/blob/main/mDNSShared/dns_sd.h>
//! (DNSServiceQueryRecord/ProcessResult/RefDeallocate). No resolver files,
//! hard-coded DNS servers, multicast browsing, or detached worker threads.
use hickory_resolver::proto::{
    rr::rdata::SRV,
    serialize::binary::{BinDecodable, BinDecoder},
};
use std::io;

const MORE_COMING: u32 = 1;
const ADD: u32 = 2;
const NO_SUCH_RECORD: i32 = -65554;
const SRV_TYPE: u16 = 33;
const INTERNET_CLASS: u16 = 1;

#[derive(Default)]
struct Reply {
    records: Vec<SRV>,
    error: Option<io::Error>,
    done: bool,
}

impl Reply {
    /// Ignore intermediate CNAMEs; finish only on an SRV batch or negative answer.
    fn accept(&mut self, flags: u32, error: i32, kind: u16, class: u16, data: &[u8]) {
        if self.done {
            return;
        }
        if error != 0 {
            if error != NO_SUCH_RECORD {
                self.error = Some(dns_error(error));
            }
            self.done = true;
            return;
        }
        if kind != SRV_TYPE || class != INTERNET_CLASS {
            return;
        }
        let mut decoder = BinDecoder::new(data);
        match SRV::read(&mut decoder) {
            Ok(record) if decoder.is_empty() => {
                if flags & ADD != 0 {
                    if !self.records.contains(&record) {
                        // A malformed daemon/response cannot grow this one-shot query indefinitely.
                        if self.records.len() >= 256 {
                            self.error = Some(io::Error::other("Too many Java SRV records"));
                        } else {
                            self.records.push(record);
                        }
                    }
                } else {
                    self.records.retain(|existing| *existing != record);
                }
            }
            _ => {
                self.error = Some(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid Java SRV record",
                ))
            }
        }
        self.done = self.error.is_some() || flags & MORE_COMING == 0;
    }
}

fn dns_error(code: i32) -> io::Error {
    io::Error::other(format!("Apple DNS SRV query failed ({code})"))
}

#[cfg(target_os = "ios")]
#[path = "apple_dns/native.rs"]
mod native;
#[cfg(target_os = "ios")]
pub(super) use native::lookup;

#[cfg(test)]
mod tests {
    use super::*;

    // priority=10, weight=20, port=25566, target=mc.test. RFC 2782 wire format.
    const RECORD: &[u8] = b"\x00\x0a\x00\x14\x63\xde\x02mc\x04test\x00";

    #[test]
    fn srv_batch_decodes_and_deduplicates_without_finishing_on_cname() {
        let mut reply = Reply::default();
        reply.accept(ADD, 0, 5, INTERNET_CLASS, b"intermediate alias");
        assert!(!reply.done);
        reply.accept(ADD | MORE_COMING, 0, SRV_TYPE, INTERNET_CLASS, RECORD);
        assert!(!reply.done);
        reply.accept(ADD, 0, SRV_TYPE, INTERNET_CLASS, RECORD);
        assert!(reply.done);
        assert!(reply.error.is_none());
        assert_eq!(reply.records.len(), 1);
        let record = &reply.records[0];
        assert_eq!(
            (record.priority(), record.weight(), record.port()),
            (10, 20, 25566)
        );
        assert_eq!(record.target().to_utf8(), "mc.test.");
    }

    #[test]
    fn only_absence_allows_default_port_fallback() {
        let mut reply = Reply::default();
        reply.accept(0, NO_SUCH_RECORD, 0, 0, &[]);
        assert!(reply.done && reply.error.is_none() && reply.records.is_empty());
        let mut reply = Reply::default();
        reply.accept(0, -65563, 0, 0, &[]);
        assert!(reply.done && reply.error.is_some());
    }

    #[test]
    fn malformed_records_fail_and_removals_do_not_leave_stale_targets() {
        for data in [
            &RECORD[..5],
            &[RECORD, b"extra"].concat(),
            b"\0\0\0\0\0\0\xc0\xff",
        ] {
            let mut reply = Reply::default();
            reply.accept(ADD, 0, SRV_TYPE, INTERNET_CLASS, data);
            assert!(reply.done && reply.error.is_some());
        }
        let mut reply = Reply::default();
        reply.accept(ADD | MORE_COMING, 0, SRV_TYPE, INTERNET_CLASS, RECORD);
        reply.accept(0, 0, SRV_TYPE, INTERNET_CLASS, RECORD);
        assert!(reply.done && reply.records.is_empty() && reply.error.is_none());
    }
}
