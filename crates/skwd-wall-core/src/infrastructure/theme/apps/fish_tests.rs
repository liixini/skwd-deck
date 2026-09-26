use super::{
    files, manager,
    tests::{fixture, palette},
};
use std::io::{Read, Write};
use std::os::fd::FromRawFd;
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::time::{Duration, Instant};

#[test]
#[ignore = "requires SKWD_TEST_FISH pointing to the real Fish executable"]
fn fish_sessions_receive_colours_and_restore_global_and_universal_settings() {
    let (_root, mut env, config) = fixture();
    let fish = std::env::var_os("SKWD_TEST_FISH").expect("Set SKWD_TEST_FISH");
    std::fs::remove_file(env.search[0].join("fish")).unwrap();
    std::os::unix::fs::symlink(&fish, env.search[0].join("fish")).unwrap();
    env.reload = true;
    std::fs::create_dir_all(&env.home).unwrap();
    let path = env.config.join("fish/config.fish");
    let original = "set -gx fish_color_command 112233 --bold\nset -U fish_color_quote aabbcc\nset -eg fish_color_quote\nset -g fish_color_error\n";
    files::write(&path, original).unwrap();
    manager::set_with(&env, &config, "fish", true, &palette("#123456"), true).unwrap();
    let capture = env.home.join("observed");
    let errors = std::fs::File::create(env.home.join("stderr")).unwrap();
    let mut master = -1;
    let mut slave = -1;
    assert_eq!(
        unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
            )
        },
        0
    );
    let mut input = unsafe { std::fs::File::from_raw_fd(master) };
    let terminal = unsafe { std::fs::File::from_raw_fd(slave) };
    let mut reader = input.try_clone().unwrap();
    std::thread::spawn(move || {
        let mut errors = errors;
        let mut bytes = [0; 4096];
        while let Ok(n) = reader.read(&mut bytes) {
            if n == 0 {
                break;
            }
            let _ = errors.write_all(&bytes[..n]);
            if bytes[..n].windows(3).any(|part| part == b"[0c") {
                let _ = reader.write_all(b"\x1b[?1;2c");
            }
        }
    });
    let mut command = Command::new(fish);
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command
        .arg("--interactive")
        .args(["--init-command", "function observe --on-variable fish_color_command --on-event fish_prompt; printf '%s\\n' $fish_color_command > $CAPTURE; end"])
        .env("HOME", &env.home)
        .env("XDG_CONFIG_HOME", &env.config)
        .env("TERM", "xterm-256color")
        .env("CAPTURE", &capture)
        .stdout(terminal.try_clone().unwrap())
        .stderr(terminal.try_clone().unwrap())
        .stdin(terminal)
        .spawn()
        .unwrap();
    let wait = |expected: &str| {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if std::fs::read_to_string(&capture).is_ok_and(|text| text == expected) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "expected {expected:?}, got {:?}; stderr {:?}",
                std::fs::read_to_string(&capture),
                std::fs::read_to_string(env.home.join("stderr"))
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    wait("#123456\n");
    manager::apply_with(&env, &config, &palette("#abcdef"), false);
    let receipt = files::load(&env.receipts.join("fish.json")).unwrap().unwrap();
    assert_ne!(receipt.result, "reload-needed");
    wait("#abcdef\n");
    manager::set_with(&env, &config, "fish", false, &serde_json::Value::Null, true).unwrap();
    wait("112233\n--bold\n");
    writeln!(input, "functions -e observe; set -q -gx fish_color_command; and set -q -g fish_color_error; and test (count $fish_color_error) -eq 0; and not set -q -g fish_color_quote; and test $fish_color_quote = aabbcc; and not functions -q __skwd_refresh_theme; and echo restored > $CAPTURE; exit").unwrap();
    wait("restored\n");
    assert!(child.wait().unwrap().success());
    assert_eq!(files::read(&path).unwrap().unwrap(), original);
}
