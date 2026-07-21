//! Persist pet state under `$CODEX_HOME/ccpet/`.

use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

use super::state::PetState;

pub(crate) fn ccpet_dir(codex_home: &Path) -> PathBuf {
    codex_home.join("ccpet")
}

pub(crate) fn pet_state_path(codex_home: &Path) -> PathBuf {
    ccpet_dir(codex_home).join("pet-state.json")
}

pub(crate) fn animation_counter_path(codex_home: &Path) -> PathBuf {
    ccpet_dir(codex_home).join("animation-counter.json")
}

pub(crate) fn graveyard_dir(codex_home: &Path) -> PathBuf {
    ccpet_dir(codex_home).join("graveyard")
}

pub(crate) fn load_state(codex_home: &Path) -> Option<PetState> {
    let path = pet_state_path(codex_home);
    let data = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&data).ok()
}

pub(crate) fn save_state(codex_home: &Path, state: &PetState) -> io::Result<()> {
    let dir = ccpet_dir(codex_home);
    fs::create_dir_all(&dir)?;
    let path = pet_state_path(codex_home);
    let data = serde_json::to_string_pretty(state)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, data)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

/// Move the current pet into the graveyard and remove the live state file.
pub(crate) fn move_to_graveyard(codex_home: &Path, state: &PetState) -> io::Result<()> {
    let grave = graveyard_dir(codex_home);
    fs::create_dir_all(&grave)?;

    let base_name = sanitize_name(&state.pet_name);
    let mut dir = grave.join(&base_name);
    let mut n = 1u32;
    while dir.exists() {
        dir = grave.join(format!("{base_name}-{n}"));
        n += 1;
    }
    fs::create_dir_all(&dir)?;
    let dest = dir.join("pet-state.json");
    let data = serde_json::to_string_pretty(state)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    fs::write(dest, data)?;

    let live = pet_state_path(codex_home);
    if live.exists() {
        let _ = fs::remove_file(live);
    }
    Ok(())
}

pub(crate) fn load_animation_frame(codex_home: &Path) -> u64 {
    let path = animation_counter_path(codex_home);
    let Ok(data) = fs::read_to_string(path) else {
        return 0;
    };
    #[derive(serde::Deserialize)]
    struct Counter {
        call_count: u64,
    }
    serde_json::from_str::<Counter>(&data)
        .map(|c| c.call_count)
        .unwrap_or(0)
}

pub(crate) fn save_animation_frame(codex_home: &Path, call_count: u64) {
    let dir = ccpet_dir(codex_home);
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = animation_counter_path(codex_home);
    let data = serde_json::json!({ "callCount": call_count, "call_count": call_count });
    let _ = fs::write(path, data.to_string());
}

fn sanitize_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "pet".to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use chrono::Utc;

    #[test]
    fn save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = PetState::new_default(Utc.with_ymd_and_hms(2026, 7, 20, 0, 0, 0).unwrap());
        state.energy = 42.5;
        state.accumulated_tokens = 123;
        save_state(dir.path(), &state).unwrap();
        let loaded = load_state(dir.path()).unwrap();
        assert!((loaded.energy - 42.5).abs() < 1e-9);
        assert_eq!(loaded.accumulated_tokens, 123);
        assert_eq!(loaded.pet_name, state.pet_name);
    }

    #[test]
    fn graveyard_move() {
        let dir = tempfile::tempdir().unwrap();
        let state = PetState::new_default(Utc.with_ymd_and_hms(2026, 7, 20, 0, 0, 0).unwrap());
        save_state(dir.path(), &state).unwrap();
        move_to_graveyard(dir.path(), &state).unwrap();
        assert!(load_state(dir.path()).is_none());
        let grave = graveyard_dir(dir.path());
        assert!(grave.exists());
        let entries: Vec<_> = fs::read_dir(grave).unwrap().collect();
        assert_eq!(entries.len(), 1);
    }
}
