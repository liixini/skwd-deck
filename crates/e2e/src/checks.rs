#[derive(Default)]
pub struct Checks {
    fails: Vec<String>,
    count: usize,
}

impl Checks {
    pub fn check(&mut self, name: &str, ok: bool, detail: impl FnOnce() -> String) {
        self.count += 1;
        if ok {
            eprintln!("  ok    {name}");
        } else {
            let why = detail();
            eprintln!("  FAIL  {name}  [{why}]");
            self.fails.push(format!("{name}: {why}"));
        }
    }

    pub fn failed(&self) -> bool {
        !self.fails.is_empty()
    }

    pub fn finish(self) {
        eprintln!("{}/{} checks passed", self.count - self.fails.len(), self.count);
        assert!(
            self.fails.is_empty(),
            "{} check(s) failed:\n{}",
            self.fails.len(),
            self.fails.join("\n")
        );
    }
}
