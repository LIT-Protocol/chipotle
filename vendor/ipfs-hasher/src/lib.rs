//! CIDv0 UnixFS hashing. Based on ipfs-hasher 0.13.0 (MIT), Wilfried Kopp.
//! Retain blocks emitted during push: at an exact chunk boundary, finish can
//! legitimately emit no further blocks. Discarding push's final root panics.
use ipfs_unixfs::file::adder::FileAdder;

#[derive(Debug, Default)]
pub struct IpfsHasher {
    _private: (),
}
impl IpfsHasher {
    pub fn compute(&self, content: &[u8]) -> String {
        let mut adder = FileAdder::default();
        let mut written = 0;
        // The UnixFS empty-file root is the initial state. Every nonempty
        // file emits a leaf/root in push or finish; finish also handles empty.
        let mut root = "QmbFMke1KXqnYyBBWxB74N4c5SBnJMVAiMNRcGu6x1AwQH".to_string();
        while written < content.len() {
            let (blocks, pushed) = adder.push(&content[written..]);
            for (cid, _) in blocks {
                root = cid.to_string();
            }
            written += pushed;
        }
        for (cid, _) in adder.finish() {
            root = cid.to_string();
        }
        root
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_boundary_produces_a_cid() {
        for size in [0, 1, 262143, 262144, 262145, 524288, 524289] {
            let cid = IpfsHasher::default().compute(&vec![b'x'; size]);
            assert!(cid.starts_with("Qm"));
            assert_eq!(cid.len(), 46);
        }
        assert_eq!(
            IpfsHasher::default().compute(b"foobar\n"),
            "QmRgutAxd8t7oGkSm4wmeuByG6M51wcTso6cubDdQtuEfL"
        );
    }
}
