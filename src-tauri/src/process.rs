use std::{
    process::{Command, Stdio},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

const PROCESS_SNAPSHOT_TTL: Duration = Duration::from_millis(750);

struct ProcessSnapshot {
    captured_at: Option<Instant>,
    tasklist_csv: String,
}

static PROCESS_SNAPSHOT: OnceLock<Mutex<ProcessSnapshot>> = OnceLock::new();

fn snapshot_contains(tasklist_csv: &str, image_name: &str) -> bool {
    let image_name = image_name.trim().trim_matches('"').to_ascii_lowercase();
    !image_name.is_empty() && tasklist_csv.contains(&format!("\"{image_name}\""))
}

pub fn is_process_running(image_name: &str) -> bool {
    let cache = PROCESS_SNAPSHOT.get_or_init(|| {
        Mutex::new(ProcessSnapshot {
            captured_at: None,
            tasklist_csv: String::new(),
        })
    });
    let Ok(mut snapshot) = cache.lock() else {
        return false;
    };

    let fresh = snapshot
        .captured_at
        .is_some_and(|captured_at| captured_at.elapsed() <= PROCESS_SNAPSHOT_TTL);
    if !fresh {
        let output = Command::new("tasklist")
            .args(["/FO", "CSV", "/NH"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();

        if let Ok(output) = output {
            snapshot.tasklist_csv = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
            snapshot.captured_at = Some(Instant::now());
        }
    }

    snapshot_contains(&snapshot.tasklist_csv, image_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_exact_process_image_field() {
        let csv = "\"game.exe\",\"123\",\"Console\"\n\"helper.exe\",\"456\",\"Console\"";
        assert!(snapshot_contains(csv, "GAME.EXE"));
        assert!(!snapshot_contains(csv, "ame.exe"));
    }
}
