use bytes::Buf;

pub fn read_str<'a>(buf: &'a [u8]) -> &'a str {
    std::str::from_utf8(buf).unwrap()
}

pub fn main() {
    let mut payload = b"hello world".as_slice();
    let s = read_str(&payload);
    payload.advance(5);
    println!("{}", s);
}
