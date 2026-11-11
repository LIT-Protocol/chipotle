use hickory_proto::{
    op::Message,
    rr::{Name, RData, RecordType},
    serialize::{
        binary::BinDecoder,
        txt::{Parser, RDataParser},
    },
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

struct MeasuringAllocator;
static MAX_ALLOC: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for MeasuringAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        MAX_ALLOC.fetch_max(layout.size(), Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: MeasuringAllocator = MeasuringAllocator;

// One test keeps the allocation measurement isolated from other tests.
#[test]
fn security_backports() {
    for kind in [RecordType::SVCB, RecordType::HTTPS] {
        for token in ["alpn=\"h", "alpn=\""] {
            assert!(RData::parse(kind, ["1", ".", token].into_iter(), None).is_err());
        }
        for value in ["\"h", "\""] {
            let zone = format!("example. 60 IN {kind} 1 . alpn={value}\n");
            assert!(Parser::new(&zone, None, Some(Name::root()))
                .parse()
                .is_err());
        }
        let valid = format!("example. 60 IN {kind} 1 . alpn=h2\n");
        assert!(Parser::new(&valid, None, Some(Name::root()))
            .parse()
            .is_ok());
    }
    // The old lexer panics before it can reject this overlong domain name.
    let zone = format!("{} 60 IN A 192.0.2.1\n", "a".repeat(5000));
    assert!(Parser::new(&zone, None, Some(Name::root()))
        .parse()
        .is_err());

    // TXT accepts a long token and splits it into wire-sized character strings.
    let txt = format!("\"{}\"", "a".repeat(5000));
    assert!(RData::try_from_str(RecordType::TXT, &txt).is_ok());

    MAX_ALLOC.store(0, Ordering::Relaxed);
    let result = Message::read_queries(&mut BinDecoder::new(&[]), 65535);
    let largest = MAX_ALLOC.load(Ordering::Relaxed);
    assert!(result.is_err());
    assert!(
        largest < 64 * 1024,
        "read_queries allocated {largest} bytes"
    );

    for section in 0..4 {
        let mut packet = [0u8; 12];
        packet[4 + section * 2] = 255;
        packet[5 + section * 2] = 255;
        MAX_ALLOC.store(0, Ordering::Relaxed);
        let result = Message::from_vec(&packet);
        let largest = MAX_ALLOC.load(Ordering::Relaxed);
        assert!(result.is_err());
        assert!(
            largest < 64 * 1024,
            "section {section} allocated {largest} bytes"
        );
    }
}
