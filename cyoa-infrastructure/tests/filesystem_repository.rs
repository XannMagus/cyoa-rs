#![cfg(target_os = "linux")]
use cyoa_application::{cancellation::CancellationSource, persistence::*};
use cyoa_infrastructure::persistence::{codec, repository::LocalRepository};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
};

fn snapshot() -> SaveSnapshot {
    let id = SaveId::new("harbour-0123456789abcdef0123456789abcdef").unwrap();
    codec::decode(
        include_bytes!("fixtures/saves/v1-full-story.json"),
        &id,
        SaveCopy::Primary,
    )
    .unwrap()
    .snapshot
}

#[test]
fn missing_and_unsafe_storage_paths_are_errors_without_following_links() {
    assert!(LocalRepository::new("relative".into()).is_err());
    let root = tempfile::tempdir().unwrap();
    let mut repo = LocalRepository::new(root.path().join("data")).unwrap();
    let token = CancellationSource::default().token();
    assert!(
        repo.list(SavePage::new(None, 100).unwrap(), &token)
            .unwrap()
            .entries
            .is_empty()
    );
    assert!(!root.path().join("data").exists());
    fs::create_dir(root.path().join("data")).unwrap();
    symlink(root.path(), root.path().join("data/saves")).unwrap();
    assert_eq!(
        repo.create(snapshot(), &token).unwrap_err().visibility,
        WriteVisibility::Unchanged
    );
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn atomic_create_replace_and_independent_writer_conflict_preserve_exact_backup() {
    let root = tempfile::tempdir().unwrap();
    let mut repo = LocalRepository::new(root.path().join("data")).unwrap();
    let token = CancellationSource::default().token();
    let first = repo.create(snapshot(), &token).unwrap();
    let dir = root.path().join("data/saves");
    let primary = dir.join(format!("{}.json", first.metadata.id.as_str()));
    let backup = dir.join(format!("{}.json.bak", first.metadata.id.as_str()));
    let old = fs::read(&primary).unwrap();
    assert!(!backup.exists());
    assert_eq!(
        fs::metadata(&primary).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
        0o700
    );
    let loaded = repo
        .load(&first.metadata.id, SaveCopy::Primary, &token)
        .unwrap();
    assert_eq!(loaded.snapshot, snapshot());
    let mut other = LocalRepository::new(root.path().join("data")).unwrap();
    let stale = other
        .load(&first.metadata.id, SaveCopy::Primary, &token)
        .unwrap();
    let target = SaveTarget {
        id: first.metadata.id.clone(),
        expected_stamp: first.stamp,
    };
    let second = repo.replace(target, snapshot(), &token).unwrap();
    assert_eq!(second.metadata.revision.get(), 2);
    assert_eq!(fs::read(&backup).unwrap(), old);
    assert_eq!(
        other
            .replace(
                SaveTarget {
                    id: stale.metadata.id,
                    expected_stamp: stale.stamp
                },
                snapshot(),
                &token
            )
            .unwrap_err()
            .kind,
        StorageFailureKind::Conflict
    );
    assert_eq!(
        repo.load(&second.metadata.id, SaveCopy::Backup, &token)
            .unwrap()
            .metadata
            .revision
            .get(),
        1
    );
    let listing = repo
        .list(SavePage::new(None, 100).unwrap(), &token)
        .unwrap();
    assert_eq!(listing.entries.len(), 1);
    assert!(listing.entries[0].backup_available);
    assert!(
        !dir.join(format!("{}.assets", second.metadata.id.as_str()))
            .exists()
    );
}

#[test]
fn corrupt_primary_never_rotates_over_a_good_backup_and_recovery_creates_a_new_slot() {
    let root = tempfile::tempdir().unwrap();
    let mut repo = LocalRepository::new(root.path().to_path_buf()).unwrap();
    let token = CancellationSource::default().token();
    let first = repo.create(snapshot(), &token).unwrap();
    repo.replace(
        SaveTarget {
            id: first.metadata.id.clone(),
            expected_stamp: first.stamp,
        },
        snapshot(),
        &token,
    )
    .unwrap();
    let path = root
        .path()
        .join("saves")
        .join(format!("{}.json", first.metadata.id.as_str()));
    let backup = path.with_extension("json.bak");
    let good = fs::read(&backup).unwrap();
    fs::write(&path, b"corrupt").unwrap();
    let error = repo
        .replace(
            SaveTarget {
                id: first.metadata.id.clone(),
                expected_stamp: codec::stamp(b"corrupt"),
            },
            snapshot(),
            &token,
        )
        .unwrap_err();
    assert!(matches!(error.kind, StorageFailureKind::Corrupt { .. }));
    assert_eq!(fs::read(&backup).unwrap(), good);
    let recovered = repo
        .load(&first.metadata.id, SaveCopy::Backup, &token)
        .unwrap();
    let receipt = repo.create(recovered.snapshot, &token).unwrap();
    assert_ne!(receipt.metadata.id, first.metadata.id);
    assert_eq!(fs::read(&path).unwrap(), b"corrupt");
    fs::remove_file(&path).unwrap();
    let listing = repo
        .list(SavePage::new(None, 100).unwrap(), &token)
        .unwrap();
    assert!(
        listing
            .entries
            .iter()
            .any(|e| e.id == first.metadata.id && e.status == SaveListingStatus::BackupOnly)
    );
}

#[test]
fn locks_survive_primary_replacement_and_hostile_entries_are_never_followed() {
    use rustix::fs::{FlockOperation, flock};
    let root = tempfile::tempdir().unwrap();
    let mut repo = LocalRepository::new(root.path().into()).unwrap();
    let token = CancellationSource::default().token();
    let receipt = repo.create(snapshot(), &token).unwrap();
    let dir = root.path().join("saves");
    let primary = dir.join(format!("{}.json", receipt.metadata.id.as_str()));
    let lock = dir.join(format!("{}.lock", receipt.metadata.id.as_str()));
    let locked = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock)
        .unwrap();
    flock(&locked, FlockOperation::NonBlockingLockExclusive).unwrap();
    assert_eq!(
        repo.load(&receipt.metadata.id, SaveCopy::Primary, &token)
            .unwrap_err()
            .kind,
        StorageFailureKind::Busy
    );
    assert_eq!(
        repo.list(SavePage::new(None, 100).unwrap(), &token)
            .unwrap()
            .entries[0]
            .status,
        SaveListingStatus::Busy
    );
    drop(locked);
    let lock_meta = fs::metadata(&lock).unwrap();
    repo.replace(
        SaveTarget {
            id: receipt.metadata.id.clone(),
            expected_stamp: receipt.stamp,
        },
        snapshot(),
        &token,
    )
    .unwrap();
    use std::os::unix::fs::MetadataExt;
    assert_eq!(fs::metadata(&lock).unwrap().ino(), lock_meta.ino());
    let bytes = fs::read(&primary).unwrap();
    let external = root.path().join("external");
    fs::write(&external, &bytes).unwrap();
    for mode in 0..3 {
        fs::remove_file(&primary).unwrap();
        match mode {
            0 => symlink(&external, &primary).unwrap(),
            1 => fs::hard_link(&external, &primary).unwrap(),
            _ => fs::create_dir(&primary).unwrap(),
        };
        assert!(
            repo.load(&receipt.metadata.id, SaveCopy::Primary, &token)
                .is_err()
        );
        assert_eq!(fs::read(&external).unwrap(), bytes);
        if mode == 2 {
            fs::remove_dir(&primary).unwrap();
        } else {
            fs::remove_file(&primary).unwrap();
        }
        fs::write(&primary, &bytes).unwrap();
    }
    for path in [lock, primary.with_extension("json.bak")] {
        fs::remove_file(&path).unwrap();
        symlink(&external, &path).unwrap();
        assert!(
            repo.replace(
                SaveTarget {
                    id: receipt.metadata.id.clone(),
                    expected_stamp: codec::stamp(&bytes)
                },
                snapshot(),
                &token
            )
            .is_err()
        );
        fs::remove_file(&path).unwrap();
        fs::write(
            &path,
            if path.extension().unwrap() == "lock" {
                b"".as_slice()
            } else {
                &bytes
            },
        )
        .unwrap();
    }
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o777)).unwrap();
    assert!(
        repo.load(&receipt.metadata.id, SaveCopy::Primary, &token)
            .is_err()
    );
}

#[test]
fn listing_pages_report_corrupt_future_and_backup_only_without_rewriting_documents() {
    let root = tempfile::tempdir().unwrap();
    let mut repo = LocalRepository::new(root.path().into()).unwrap();
    let token = CancellationSource::default().token();
    for _ in 0..3 {
        repo.create(snapshot(), &token).unwrap();
    }
    let rows = repo
        .list(SavePage::new(None, 100).unwrap(), &token)
        .unwrap()
        .entries;
    let dir = root.path().join("saves");
    let path = |n: usize| dir.join(format!("{}.json", rows[n].id.as_str()));
    fs::write(path(0), b"bad").unwrap();
    let future = b"{\"version\":2}";
    fs::write(path(1), future).unwrap();
    fs::rename(path(2), path(2).with_extension("json.bak")).unwrap();
    fs::write(dir.join(".cyoa-unrelated"), b"not a save").unwrap();
    let mut after = None;
    let mut statuses = vec![];
    loop {
        let result = repo.list(SavePage::new(after, 1).unwrap(), &token).unwrap();
        assert_eq!(result.entries.len(), 1);
        statuses.push(result.entries[0].status.clone());
        after = result.next;
        if after.is_none() {
            break;
        }
    }
    assert_eq!(
        statuses,
        vec![
            SaveListingStatus::Corrupt,
            SaveListingStatus::FutureVersion { version: 2 },
            SaveListingStatus::BackupOnly
        ]
    );
    assert_eq!(fs::read(path(0)).unwrap(), b"bad");
    assert_eq!(fs::read(path(1)).unwrap(), future);
}

#[test]
fn disk_restore_cycles_preserve_original_limits_and_every_narrowed_snapshot() {
    use cyoa_core::{game::TurnCount, limits::*};
    let root = tempfile::tempdir().unwrap();
    let repo = LocalRepository::new(root.path().into()).unwrap();
    let mut cases = PersistenceUseCases::new(repo);
    let token = CancellationSource::default().token();
    let receipt = cases
        .save_game(SaveGame::Create(snapshot()), &token)
        .unwrap();
    let current = Limits {
        max_major_events: MajorEventLimit::new(1).unwrap(),
        max_generated_npcs: MaxGeneratedNpcs::new(0),
        prose_bridge_turns: ProseBridgeTurns::new(0),
        min_playable_characters: MinPlayableCharacters::new(9).unwrap(),
    };
    let loaded = cases
        .load_game(
            LoadGame {
                id: receipt.metadata.id.clone(),
                copy: SaveCopy::Primary,
                limits: RestoreLimits::Current(current),
            },
            &token,
        )
        .unwrap();
    assert!(loaded.changed_by_restore);
    let narrowed = loaded.stored.snapshot;
    assert_eq!(narrowed.game.world().cast(), snapshot().game.world().cast());
    let saved = cases
        .save_game(
            SaveGame::Replace {
                target: SaveTarget {
                    id: receipt.metadata.id.clone(),
                    expected_stamp: receipt.stamp,
                },
                snapshot: narrowed.clone(),
            },
            &token,
        )
        .unwrap();
    let mut original = cases
        .load_game(
            LoadGame {
                id: receipt.metadata.id.clone(),
                copy: SaveCopy::Primary,
                limits: RestoreLimits::Original,
            },
            &token,
        )
        .unwrap()
        .stored
        .snapshot;
    assert_eq!(
        original.game.original_limits(),
        snapshot().game.original_limits()
    );
    for (before, after) in narrowed.game.turns().iter().zip(original.game.turns()) {
        assert_eq!(
            before.summary().major_events().events(),
            after.summary().major_events().events()
        );
        assert_eq!(after.summary().major_events().limit().get(), 4);
    }
    original.game.rewind(TurnCount::new(1).unwrap()).unwrap();
    let final_receipt = cases
        .save_game(
            SaveGame::Replace {
                target: SaveTarget {
                    id: saved.metadata.id.clone(),
                    expected_stamp: saved.stamp,
                },
                snapshot: original.clone(),
            },
            &token,
        )
        .unwrap();
    let inspected = cases
        .inspect_save(
            InspectSave {
                id: final_receipt.metadata.id,
                copy: SaveCopy::Primary,
            },
            &token,
        )
        .unwrap();
    assert_eq!(inspected.snapshot, original);
}
