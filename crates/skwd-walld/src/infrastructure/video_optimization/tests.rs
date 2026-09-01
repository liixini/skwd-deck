use super::*;

#[test]
fn tinier_encode_flags() {
    let args = tinier_encode_args("/in.mp4", "/out.ivf", 1440, Some(30));
    assert!(args.windows(2).any(|pair| pair == ["-c:v", "libsvtav1"]));
    assert!(args.windows(2).any(|pair| pair == ["-preset", "12"]));
    assert!(args.windows(2).any(|pair| pair == ["-svtav1-params", "lp=2:pred-struct=1"]));
    assert!(args.windows(2).any(|pair| pair == ["-f", "ivf"]));
    assert!(args.windows(2).any(|pair| pair == ["-progress", "pipe:1"]));
    assert!(args.contains(&String::from("-nostats")));
    assert!(args.windows(2).any(|pair| pair == ["-vf", "fps=30,scale=-2:'min(ih,1440)'"]));
    assert!(args.contains(&String::from("-an")));
    assert!(!args.contains(&String::from("copy")));
    assert_eq!(args.last().unwrap(), "/out.ivf");
}

#[test]
fn tinier_dest_stable() {
    let first = tinier_dest_path(std::path::Path::new("/cache"), "/video/a.mp4");
    let same = tinier_dest_path(std::path::Path::new("/cache"), "/video/a.mp4");
    let other = tinier_dest_path(std::path::Path::new("/cache"), "/other/a.mp4");
    assert_eq!(first, same);
    assert_ne!(first, other);
    assert!(first.starts_with("/cache/video-opt"));
    assert!(first.to_string_lossy().ends_with(".tinier-v1.ivf"));
}

#[test]
fn tinier_input_flags() {
    let args = tinier_encode_args("/in.mp4", "/out.ivf", 1440, Some(30));
    let hardware = args.iter().position(|arg| arg == "-hwaccel").unwrap();
    let threads = args.iter().position(|arg| arg == "-threads").unwrap();
    let input = args.iter().position(|arg| arg == "-i").unwrap();
    assert_eq!(args[hardware + 1], "auto");
    assert_eq!(args[threads + 1], OPT_THREADS);
    assert!(hardware < input);
    assert!(threads < input);
}

#[test]
fn decode_probe_dav1d() {
    let args = decode_probe_args("/cache/video.tinier-v1.ivf", "av1");
    assert!(args.windows(2).any(|window| window == ["-c:v", "libdav1d"]));
    assert!(args.windows(2).any(|window| window == ["-frames:v", "1"]));
    assert!(args.windows(2).any(|window| window == ["-map", "0:v:0"]));
}

#[test]
fn frame_rate_average_first() {
    assert_eq!(
        frame_rate_from_probe("r_frame_rate=24/1\navg_frame_rate=24000/1001\n"),
        Some(String::from("24000/1001"))
    );
    assert_eq!(
        frame_rate_from_probe("r_frame_rate=24/1\navg_frame_rate=0/0\n"),
        Some(String::from("24/1"))
    );
    assert_eq!(frame_rate_from_probe("r_frame_rate=0/0\navg_frame_rate=0/0\n"), None);
}

#[test]
fn general_variant_match() {
    let root = std::path::Path::new("/cache/video-opt");
    for name in ["a.av1.mp4", "a.vp9.mp4", "a.h264-lean.mp4"] {
        assert!(is_general_variant(root, &root.join(name)));
    }
    assert!(!is_general_variant(root, &root.join("a.tinier-v1.ivf")));
    assert!(!is_general_variant(root, std::path::Path::new("/videos/a.av1.mp4")));
    assert!(!is_general_variant(root, &root.join("nested/a.av1.mp4")));
}
