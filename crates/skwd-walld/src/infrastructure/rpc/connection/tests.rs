use super::{MAX_REQUEST_BYTES, read_bounded_line};

fn run<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(future)
}

#[test]
fn bounded_reader_normal_line() {
    run(async {
        let mut reader = &b"{\"id\":1}\nnext\n"[..];
        let mut line = Vec::new();
        assert_eq!(read_bounded_line(&mut reader, &mut line).await.unwrap(), 9);
        assert_eq!(line, b"{\"id\":1}\n");
        assert_eq!(read_bounded_line(&mut reader, &mut line).await.unwrap(), 5);
        assert_eq!(line, b"next\n");
    });
}

#[test]
fn bounded_reader_oversized() {
    run(async {
        let input = vec![b'x'; MAX_REQUEST_BYTES + 1];
        let mut reader = &input[..];
        let mut line = Vec::new();
        let error = read_bounded_line(&mut reader, &mut line).await.unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(line.len() <= MAX_REQUEST_BYTES);
    });
}
