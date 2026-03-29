use audex::id3::file::clear_from_writer;
use audex::{AudexError, ReadWriteSeek};
use std::io::{Error, ErrorKind, Read, Seek, SeekFrom, Write};

struct HeaderReadErrorWriter {
    pos: u64,
    fail_kind: ErrorKind,
}

impl HeaderReadErrorWriter {
    fn new(fail_kind: ErrorKind) -> Self {
        Self { pos: 0, fail_kind }
    }
}

impl Read for HeaderReadErrorWriter {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        Err(Error::new(self.fail_kind, "injected header read failure"))
    }
}

impl Write for HeaderReadErrorWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.pos = self.pos.saturating_add(buf.len() as u64);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Seek for HeaderReadErrorWriter {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        match pos {
            SeekFrom::Start(offset) => {
                self.pos = offset;
            }
            SeekFrom::Current(delta) => {
                self.pos = self.pos.checked_add_signed(delta).ok_or_else(|| {
                    Error::new(ErrorKind::InvalidInput, "invalid seek before start")
                })?;
            }
            SeekFrom::End(0) => {
                self.pos = 10;
            }
            SeekFrom::End(delta) => {
                self.pos = 10u64.checked_add_signed(delta).ok_or_else(|| {
                    Error::new(ErrorKind::InvalidInput, "invalid seek before start")
                })?;
            }
        }
        Ok(self.pos)
    }
}

fn assert_io_error(result: audex::Result<()>, expected_kind: ErrorKind) {
    match result {
        Err(AudexError::Io(err)) => assert_eq!(err.kind(), expected_kind),
        other => panic!("expected io error, got {other:?}"),
    }
}

#[test]
fn clear_from_writer_propagates_non_eof_header_errors() {
    let mut writer = HeaderReadErrorWriter::new(ErrorKind::PermissionDenied);
    assert_io_error(
        clear_from_writer(&mut writer as &mut dyn ReadWriteSeek, false, true),
        ErrorKind::PermissionDenied,
    );
}

#[test]
fn clear_from_writer_treats_short_inputs_as_noop() {
    let mut writer = std::io::Cursor::new(vec![0u8; 5]);
    clear_from_writer(&mut writer, false, true).expect("short inputs should be ignored");
}
