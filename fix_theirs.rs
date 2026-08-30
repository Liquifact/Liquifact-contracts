use std::fs;

fn main() {
    let files = ["escrow/src/lib.rs", "escrow/src/keys.rs"];
    for file in files {
        let mut bytes = fs::read(file).unwrap();
        if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
            bytes = bytes[3..].to_vec();
        }
        let text = String::from_utf8_lossy(&bytes);
        let mut new_text = String::new();
        let mut in_conflict = false;
        let mut keeping = true;
        for line in text.lines() {
            if line.starts_with("<<<<<<<") {
                in_conflict = true;
                keeping = false; // DROP HEAD
            } else if line.starts_with("=======") && in_conflict {
                keeping = true; // KEEP THEIRS
            } else if line.starts_with(">>>>>>>") && in_conflict {
                in_conflict = false;
                keeping = true;
            } else if line.starts_with("=======") {
                continue;
            } else if line.starts_with(">>>>>>>") {
                continue;
            } else {
                if keeping {
                    new_text.push_str(line);
                    new_text.push('\n');
                }
            }
        }
        fs::write(file, new_text).unwrap();
    }
}
