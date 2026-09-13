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

fn tasklist_image_name(line: &str) -> Option<&str> {
    let line = line.trim_start();
    let quoted = line.strip_prefix('"')?;
    let end = quoted.find("\",")?;
    let value = &quoted[..end];
    (!value.is_empty()).then_some(value)
}

fn snapshot_contains(tasklist_csv: &str, image_name: &str) -> bool {
    let expected = image_name.trim().trim_matches('"');
    !expected.is_empty()
        && tasklist_csv.lines().any(|line| {
            tasklist_image_name(line)
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(expected))
        })
}

fn capture_tasklist_csv() -> Option<String> {
    let output = Command::new("tasklist")
        .args(["/FO", "CSV", "/NH"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;

    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
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
        snapshot.tasklist_csv = capture_tasklist_csv().unwrap_or_default();
        snapshot.captured_at = Some(Instant::now());
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

    #[test]
    fn does_not_match_values_from_other_csv_columns() {
        let csv = "\"helper.exe\",\"123\",\"game.exe\"";
        assert!(!snapshot_contains(csv, "game.exe"));
    }

    #[test]
    fn parses_image_names_with_commas() {
        let csv = "\"game,modded.exe\",\"123\",\"Console\"";
        assert_eq!(tasklist_image_name(csv), Some("game,modded.exe"));
        assert!(snapshot_contains(csv, "game,modded.exe"));
    }

    #[test]
    fn ignores_malformed_tasklist_rows() {
        let csv = "not csv\n\"valid.exe\",\"456\",\"Console\"";
        assert!(snapshot_contains(csv, "valid.exe"));
        assert!(!snapshot_contains(csv, "not csv"));
    }
}
