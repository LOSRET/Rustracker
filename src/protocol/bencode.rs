pub fn write_key(buf: &mut Vec<u8>, key: &[u8]) {
    buf.extend_from_slice(key.len().to_string().as_bytes());
    buf.push(b':');
    buf.extend_from_slice(key);
}

pub fn write_int(buf: &mut Vec<u8>, key: &[u8], value: i64) {
    write_key(buf, key);
    buf.push(b'i');
    buf.extend_from_slice(value.to_string().as_bytes());
    buf.push(b'e');
}

pub fn write_bytes(buf: &mut Vec<u8>, key: &[u8], value: &[u8]) {
    write_key(buf, key);
    buf.extend_from_slice(value.len().to_string().as_bytes());
    buf.push(b':');
    buf.extend_from_slice(value);
}

pub fn write_int_raw(buf: &mut Vec<u8>, value: i64) {
    buf.push(b'i');
    buf.extend_from_slice(value.to_string().as_bytes());
    buf.push(b'e');
}

pub fn failure(message: impl AsRef<str>) -> Vec<u8> {
    let msg = message.as_ref();
    let mut buf = Vec::with_capacity(32 + msg.len());
    buf.push(b'd');
    write_bytes(&mut buf, b"failure reason", msg.as_bytes());
    buf.push(b'e');
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_failure() {
        let encoded = failure("test error");
        assert_eq!(&encoded, b"d14:failure reason10:test errore");
    }

    #[test]
    fn encodes_int() {
        let mut buf = Vec::new();
        write_int(&mut buf, b"complete", 42);
        assert_eq!(&buf, b"8:completei42e");
    }

    #[test]
    fn encodes_bytes() {
        let mut buf = Vec::new();
        write_bytes(&mut buf, b"peers", b"\x7f\x00\x00\x01\x1a\xe1");
        assert_eq!(&buf, b"5:peers6:\x7f\x00\x00\x01\x1a\xe1");
    }

    #[test]
    fn encodes_key() {
        let mut buf = Vec::new();
        write_key(&mut buf, b"interval");
        assert_eq!(&buf, b"8:interval");
    }

    #[test]
    fn encodes_int_raw() {
        let mut buf = Vec::new();
        write_int_raw(&mut buf, 1800);
        assert_eq!(&buf, b"i1800e");
    }
}
