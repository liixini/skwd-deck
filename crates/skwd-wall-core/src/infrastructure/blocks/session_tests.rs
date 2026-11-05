use super::*;
use crate::blocks::{NEAR_H, NEAR_W, near_mips};

fn fixture() -> Vec<Mip> {
    near_mips(&image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
        NEAR_W,
        NEAR_H,
        image::Rgb([80, 160, 240]),
    )))
}

fn pair() -> (Encoder, File) {
    let (client, server) = UnixStream::pair().unwrap();
    (Encoder::from_stream(client).unwrap(), File::from(OwnedFd::from(server)))
}

#[test]
fn wire_shapes() {
    let (encoder, mut server) = pair();
    let mips = fixture();
    let expected: Vec<_> = mips.iter().map(|mip| mip.rgba.clone()).collect();
    let worker = std::thread::spawn(move || {
        let received = read_request(&mut server).unwrap().unwrap();
        assert_eq!(received.len(), 5);
        let shapes: Vec<_> = received.iter().map(|mip| (mip.width, mip.height)).collect();
        assert_eq!(shapes, [(640, 360), (320, 180), (160, 92), (80, 48), (40, 24)]);
        assert_eq!(received.iter().map(|mip| mip.rgba.len()).sum::<usize>(), 1_230_080);
        for (mip, rgba) in received.iter().zip(expected) {
            assert_eq!(mip.rgba, rgba);
        }
        write_response(&mut server, &vec![7; NEAR_BLOCK_BYTES]).unwrap();
        assert!(read_request(&mut server).unwrap().is_none());
    });
    assert_eq!(encoder.encode(&mips), vec![7; NEAR_BLOCK_BYTES]);
    assert_eq!(crate::lock(&encoder.client).as_ref().unwrap().timeout, GPU_REQUEST_TIMEOUT);
    encoder.shutdown();
    worker.join().unwrap();
}

#[test]
fn invalid_requests() {
    for bytes in [header(*b"FAIL", REQUEST_BYTES), header(*REQUEST_MAGIC, REQUEST_BYTES + 1)] {
        let (client, server) = UnixStream::pair().unwrap();
        let mut client = File::from(OwnedFd::from(client));
        client.write_all(&bytes).unwrap();
        assert_eq!(
            read_request(&mut File::from(OwnedFd::from(server))).err().unwrap().kind(),
            io::ErrorKind::InvalidData
        );
    }
    let (client, server) = UnixStream::pair().unwrap();
    let mut client = File::from(OwnedFd::from(client));
    client.write_all(&header(*REQUEST_MAGIC, REQUEST_BYTES)).unwrap();
    client.write_all(&[0; SHAPE_BYTES]).unwrap();
    assert_eq!(
        read_request(&mut File::from(OwnedFd::from(server))).err().unwrap().kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn fallback_disables_worker() {
    let mips = fixture();
    let expected = Encoder::default().encode(&mips);
    for reply in [
        None,
        Some(header(*b"FAIL", NEAR_BLOCK_BYTES)),
        Some(header(*RESPONSE_MAGIC, NEAR_BLOCK_BYTES + 1)),
    ] {
        let (encoder, mut server) = pair();
        let worker = std::thread::spawn(move || {
            read_request(&mut server).unwrap().unwrap();
            if let Some(reply) = reply {
                server.write_all(&reply).unwrap();
            }
        });
        assert_eq!(encoder.encode(&mips), expected);
        assert!(crate::lock(&encoder.client).is_none());
        assert_eq!(encoder.encode(&mips), expected);
        worker.join().unwrap();
    }
}

#[test]
fn second_request_fails() {
    let (encoder, mut server) = pair();
    let worker = std::thread::spawn(move || {
        read_request(&mut server).unwrap().unwrap();
        write_response(&mut server, &vec![3; NEAR_BLOCK_BYTES]).unwrap();
        read_request(&mut server).unwrap().unwrap();
    });
    let mips = fixture();
    assert_eq!(encoder.encode(&mips), vec![3; NEAR_BLOCK_BYTES]);
    let expected = Encoder::default().encode(&mips);
    assert_eq!(encoder.encode(&mips), expected);
    assert_eq!(encoder.encode(&mips), expected);
    assert!(crate::lock(&encoder.client).is_none());
    worker.join().unwrap();
}

#[test]
fn timeout_closes_channel() {
    let mips = fixture();
    let expected = Encoder::default().encode(&mips);
    for read in [false, true] {
        let (encoder, mut server) = pair();
        crate::lock(&encoder.client).as_mut().unwrap().timeout = Duration::from_millis(100);
        let (done, finished) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            if read {
                read_request(&mut server).unwrap().unwrap();
            } else {
                finished.recv().unwrap();
            }
            let mut remaining = Vec::new();
            server.read_to_end(&mut remaining).unwrap();
            assert_eq!(remaining.is_empty(), read);
        });
        assert_eq!(encoder.encode(&mips), expected);
        assert!(crate::lock(&encoder.client).is_none());
        let _ = done.send(());
        worker.join().unwrap();
    }
}

#[test]
fn dribble_keeps_deadline() {
    let (encoder, mut server) = pair();
    crate::lock(&encoder.client).as_mut().unwrap().timeout = Duration::from_millis(100);
    let worker = std::thread::spawn(move || {
        read_request(&mut server).unwrap().unwrap();
        for byte in header(*RESPONSE_MAGIC, NEAR_BLOCK_BYTES) {
            if server.write_all(&[byte]).is_err() {
                return;
            }
            std::thread::sleep(Duration::from_millis(40));
        }
        panic!("deadline reset");
    });
    let mips = fixture();
    assert_eq!(encoder.encode(&mips), Encoder::default().encode(&mips));
    assert!(crate::lock(&encoder.client).is_none());
    worker.join().unwrap();
}

#[test]
fn parallel_requests_serialize() {
    let (encoder, mut server) = pair();
    let worker = std::thread::spawn(move || {
        for _ in 0..2 {
            read_request(&mut server).unwrap().unwrap();
            write_response(&mut server, &vec![9; NEAR_BLOCK_BYTES]).unwrap();
        }
    });
    let mips = fixture();
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..2).map(|_| scope.spawn(|| encoder.encode(&mips))).collect();
        for handle in handles {
            assert_eq!(handle.join().unwrap(), vec![9; NEAR_BLOCK_BYTES]);
        }
    });
    worker.join().unwrap();
}

#[test]
fn invalid_blocks_disable_worker() {
    let mips = fixture();
    let expected = Encoder::default().encode(&mips);
    for first_succeeds in [false, true] {
        let (encoder, mut server) = pair();
        let worker = std::thread::spawn(move || {
            if first_succeeds {
                read_request(&mut server).unwrap().unwrap();
                write_response(&mut server, &vec![3; NEAR_BLOCK_BYTES]).unwrap();
            }
            read_request(&mut server).unwrap().unwrap();
            let mut invalid = vec![3; NEAR_BLOCK_BYTES];
            invalid[NEAR_BLOCK_BYTES - near_size(BLOCK_EDGE, BLOCK_EDGE)] = 0;
            write_response(&mut server, &invalid).unwrap();
            assert!(read_request(&mut server).unwrap().is_none());
        });
        if first_succeeds {
            assert_eq!(encoder.encode(&mips), vec![3; NEAR_BLOCK_BYTES]);
        }
        assert_eq!(encoder.encode(&mips), expected);
        assert!(crate::lock(&encoder.client).is_none());
        assert_eq!(encoder.encode(&mips), expected);
        worker.join().unwrap();
    }
}
