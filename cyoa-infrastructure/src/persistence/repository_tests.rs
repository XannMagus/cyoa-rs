#[cfg(test)]
mod tests {
    use super::*;
    use cyoa_application::cancellation::CancellationSource;
    use std::sync::{Arc, Mutex};
    fn snapshot() -> SaveSnapshot {
        let id = SaveId::new("harbour-0123456789abcdef0123456789abcdef").unwrap();
        codec::decode(
            include_bytes!("../../tests/fixtures/saves/v1-full-story.json"),
            &id,
            SaveCopy::Primary,
        )
        .unwrap()
        .snapshot
    }
    struct Ops {
        fault: Option<Point>,
        seen: Arc<Mutex<Vec<Point>>>,
        random: Option<[u8; 16]>,
    }
    impl FileOps for Ops {
        fn check(&mut self, p: Point) -> io::Result<()> {
            self.seen.lock().unwrap().push(p);
            if self.fault == Some(p) {
                Err(io::Error::other(format!("injected {p:?}")))
            } else {
                Ok(())
            }
        }
        fn now(&mut self) -> io::Result<SavedAt> {
            Ok(SavedAt::new(0, 0).unwrap())
        }
        fn random(&mut self) -> io::Result<[u8; 16]> {
            self.random
                .ok_or_else(|| io::Error::other("entropy unavailable"))
        }
    }
    fn ops(fault: Option<Point>) -> (Box<dyn FileOps>, Arc<Mutex<Vec<Point>>>) {
        let seen = Arc::new(Mutex::new(vec![]));
        (
            Box::new(Ops {
                fault,
                seen: seen.clone(),
                random: Some([0; 16]),
            }),
            seen,
        )
    }
    #[test]
    fn every_write_failure_preserves_complete_files_visibility_and_retry_material() {
        for point in [
            Point::Open,
            Point::NewWrite,
            Point::NewFlush,
            Point::NewSync,
            Point::BackupWrite,
            Point::BackupFlush,
            Point::BackupSync,
            Point::BackupPersist,
            Point::BackupDirSync,
            Point::Recheck,
            Point::PrimaryPersist,
            Point::FinalSync,
            Point::Cleanup,
        ] {
            let root = tempfile::tempdir().unwrap();
            let token = CancellationSource::default().token();
            let mut repo = LocalRepository::new(root.path().into()).unwrap();
            let receipt = repo.create(snapshot(), &PreparedWriteEvidence::default(), &token).unwrap();
            let primary = root
                .path()
                .join("saves")
                .join(name(&receipt.metadata.id, SaveCopy::Primary));
            let backup = root
                .path()
                .join("saves")
                .join(name(&receipt.metadata.id, SaveCopy::Backup));
            let old = std::fs::read(&primary).unwrap();
            let pending = repo
                .prepare_replace(
                    &SaveTarget {
                        id: receipt.metadata.id.clone(),
                        expected_stamp: receipt.stamp,
                    },
                    &snapshot(),
                    &token,
                )
                .unwrap();
            repo.ops = ops(Some(point)).0;
            let error = repo
                .execute_pending(pending.clone(), &token, StorageOperation::Replace)
                .unwrap_err();
            assert_eq!(error.pending.as_deref(), Some(&pending), "{point:?}");
            let after = std::fs::read(&primary).unwrap();
            if matches!(point, Point::FinalSync | Point::Cleanup) {
                assert_eq!(after, pending.bytes(), "{point:?}");
                assert_eq!(
                    error.visibility,
                    WriteVisibility::Replaced {
                        stamp: pending.intended_stamp()
                    }
                );
                assert_eq!(std::fs::read(&backup).unwrap(), old);
            } else {
                assert_eq!(after, old, "{point:?}");
                assert_eq!(error.visibility, WriteVisibility::Unchanged);
            }
            if backup.exists() {
                assert_eq!(std::fs::read(&backup).unwrap(), old);
            }
            repo.ops = ops(None).0;
            let reconciled = repo.reconcile(pending.clone(), &PreparedWriteEvidence::default(), &token).unwrap();
            assert_eq!(reconciled.stamp, pending.intended_stamp());
            assert_eq!(std::fs::read(&backup).unwrap(), old);
            assert_eq!(std::fs::read(&primary).unwrap(), pending.bytes());
            assert!(
                !std::fs::read_dir(primary.parent().unwrap())
                    .unwrap()
                    .any(|e| e
                        .unwrap()
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".cyoa-"))
            );
        }
    }
    #[test]
    fn first_create_receipt_loss_reconciles_without_duplicate_slots_or_backup_rotation() {
        let root = tempfile::tempdir().unwrap();
        let token = CancellationSource::default().token();
        let mut repo = LocalRepository::new(root.path().into()).unwrap();
        let pending = repo.prepare_create(&snapshot(), &token).unwrap();
        repo.ops = ops(Some(Point::FinalSync)).0;
        let error = repo
            .execute_pending(pending.clone(), &token, StorageOperation::Create)
            .unwrap_err();
        assert!(matches!(error.visibility, WriteVisibility::Replaced { .. }));
        repo.ops = ops(None).0;
        let receipt = repo.reconcile(pending.clone(), &PreparedWriteEvidence::default(), &token).unwrap();
        assert_eq!(&receipt.metadata.id, pending.target());
        assert_eq!(receipt.metadata.revision.get(), 1);
        assert!(
            !root
                .path()
                .join("saves")
                .join(name(pending.target(), SaveCopy::Backup))
                .exists()
        );
        assert_eq!(
            repo.list(SavePage::new(None, 100).unwrap(), &token)
                .unwrap()
                .entries
                .len(),
            1
        );
        std::fs::write(
            root.path()
                .join("saves")
                .join(name(pending.target(), SaveCopy::Primary)),
            b"changed",
        )
        .unwrap();
        assert_eq!(
            repo.reconcile(pending, &PreparedWriteEvidence::default(), &token).unwrap_err().kind,
            StorageFailureKind::Conflict
        );
    }
    #[test]
    fn sync_order_constant_clock_collision_entropy_and_revision_exhaustion_are_observable() {
        let root = tempfile::tempdir().unwrap();
        let token = CancellationSource::default().token();
        let mut repo = LocalRepository::new(root.path().into()).unwrap();
        let (o, seen) = ops(None);
        repo.ops = o;
        let first = repo.create(snapshot(), &PreparedWriteEvidence::default(), &token).unwrap();
        assert_eq!(
            repo.create(snapshot(), &PreparedWriteEvidence::default(), &token).unwrap_err().kind,
            StorageFailureKind::Conflict
        );
        seen.lock().unwrap().clear();
        let second = repo
            .replace(
                SaveTarget {
                    id: first.metadata.id.clone(),
                    expected_stamp: first.stamp,
                },
                snapshot(),
                &PreparedWriteEvidence::default(),
                &token,
            )
            .unwrap();
        assert_eq!(first.metadata.saved_at, second.metadata.saved_at);
        assert_eq!(second.metadata.revision.get(), 2);
        let trace = seen.lock().unwrap();
        let pos = |p| trace.iter().position(|v| *v == p).unwrap();
        assert!(pos(Point::NewSync) < pos(Point::BackupSync));
        assert!(pos(Point::BackupSync) < pos(Point::BackupPersist));
        assert!(pos(Point::BackupPersist) < pos(Point::BackupDirSync));
        assert!(pos(Point::BackupDirSync) < pos(Point::PrimaryPersist));
        assert!(pos(Point::PrimaryPersist) < pos(Point::FinalSync));
        drop(trace);
        repo.ops = Box::new(Ops {
            fault: None,
            seen: seen.clone(),
            random: None,
        });
        assert_eq!(
            repo.create(snapshot(), &PreparedWriteEvidence::default(), &token).unwrap_err().stage,
            StorageStage::Prepare
        );
        let path = root
            .path()
            .join("saves")
            .join(name(&first.metadata.id, SaveCopy::Primary));
        let mut doc: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        doc["revision"] = u64::MAX.into();
        let bytes = serde_json::to_vec(&doc).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(
            repo.replace(
                SaveTarget {
                    id: first.metadata.id,
                    expected_stamp: codec::stamp(&bytes)
                },
                snapshot(),
                &PreparedWriteEvidence::default(),
                &token
            )
            .unwrap_err()
            .kind,
            StorageFailureKind::Conflict
        );
    }
    #[test]
    fn preparation_read_encode_and_cleanup_failures_keep_the_initiating_cause() {
        let root = tempfile::tempdir().unwrap();
        let token = CancellationSource::default().token();
        let mut repo = LocalRepository::new(root.path().into()).unwrap();
        let receipt = repo.create(snapshot(), &PreparedWriteEvidence::default(), &token).unwrap();
        for point in [Point::Read, Point::Encode] {
            repo.ops = ops(Some(point)).0;
            let error = repo
                .replace(
                    SaveTarget {
                        id: receipt.metadata.id.clone(),
                        expected_stamp: receipt.stamp,
                    },
                    snapshot(),
                    &PreparedWriteEvidence::default(),
                    &token,
                )
                .unwrap_err();
            assert_eq!(error.visibility, WriteVisibility::Unchanged);
            assert!(error.pending.is_none());
            assert!(error.message.contains(&format!("{point:?}")));
        }
        let error = failure(
            StorageOperation::Replace,
            StorageStage::Write,
            StorageFailureKind::Io,
            "original",
        );
        let result = combine_cleanup::<()>(
            Err(error),
            Err(io::Error::other("cleanup")),
            StorageOperation::Replace,
        )
        .unwrap_err();
        assert_eq!(&*result.message, "original");
        assert_eq!(&*result.cleanup_errors, ["cleanup"]);
    }

    struct BarrierOps {
        root: PathBuf,
        point: Point,
    }
    impl FileOps for BarrierOps {
        fn check(&mut self, point: Point) -> io::Result<()> {
            if point == self.point {
                std::fs::write(self.root.join("ready"), b"ready")?;
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(60));
                }
            }
            Ok(())
        }
    }
    // Child entrypoint for the real process-kill test below. Test-only environment,
    // never a production filesystem failpoint or a registered acceptance claim.
    fn crash_window_child() {
        let Some(root) = std::env::var_os("CYOA_TEST_CRASH_ROOT") else {
            return;
        };
        let root = PathBuf::from(root);
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join("attempt")).unwrap()).unwrap();
        let bytes = value["document"].as_str().unwrap().as_bytes().to_vec();
        let id = SaveId::new(value["id"].as_str().unwrap()).unwrap();
        let previous = if value["previous"].is_null() {
            None
        } else {
            Some(ContentStamp::new(
                serde_json::from_value(value["previous"].clone()).unwrap(),
            ))
        };
        let pending = PendingWrite::new(id, previous, codec::stamp(&bytes), bytes).unwrap();
        let point = match value["point"].as_str().unwrap() {
            "NewSync" => Point::NewSync,
            "BackupDirSync" => Point::BackupDirSync,
            "PrimaryPersist" => Point::PrimaryPersist,
            "FinalSync" => Point::FinalSync,
            "Cleanup" => Point::Cleanup,
            _ => panic!("unknown barrier"),
        };
        let mut repo = LocalRepository::new(root.join("data")).unwrap();
        repo.ops = Box::new(BarrierOps { root, point });
        let _ = repo.execute_pending(
            pending,
            &CancellationSource::default().token(),
            StorageOperation::Reconcile,
        );
        panic!("barrier was not reached");
    }
    #[test]
    fn killed_helpers_leave_complete_crash_window_files_release_locks_and_reconcile_once() {
        if std::env::var_os("CYOA_TEST_CRASH_ROOT").is_some() {
            crash_window_child();
            return;
        }
        use std::time::{Duration, Instant};
        for creating in [true, false] {
            for point in [
                Point::NewSync,
                Point::BackupDirSync,
                Point::PrimaryPersist,
                Point::FinalSync,
                Point::Cleanup,
            ] {
                if creating && point == Point::BackupDirSync {
                    continue;
                }
                let root = tempfile::tempdir().unwrap();
                let token = CancellationSource::default().token();
                let mut repo = LocalRepository::new(root.path().join("data")).unwrap();
                let (pending, old, prior_backup) = if creating {
                    (
                        repo.prepare_create(&snapshot(), &token).unwrap(),
                        None,
                        None,
                    )
                } else {
                    let first = repo.create(snapshot(), &PreparedWriteEvidence::default(), &token).unwrap();
                    let second = repo
                        .replace(
                            SaveTarget {
                                id: first.metadata.id,
                                expected_stamp: first.stamp,
                            },
                            snapshot(),
                            &PreparedWriteEvidence::default(),
                            &token,
                        )
                        .unwrap();
                    let dir = root.path().join("data/saves");
                    let old = std::fs::read(dir.join(name(&second.metadata.id, SaveCopy::Primary)))
                        .unwrap();
                    let backup =
                        std::fs::read(dir.join(name(&second.metadata.id, SaveCopy::Backup)))
                            .unwrap();
                    (
                        repo.prepare_replace(
                            &SaveTarget {
                                id: second.metadata.id,
                                expected_stamp: second.stamp,
                            },
                            &snapshot(),
                            &token,
                        )
                        .unwrap(),
                        Some(old),
                        Some(backup),
                    )
                };
                let request = serde_json::json!({"id":pending.target().as_str(),"previous":pending.previous_stamp().map(|s|*s.bytes()),"document":std::str::from_utf8(pending.bytes()).unwrap(),"point":format!("{point:?}")});
                std::fs::write(
                    root.path().join("attempt"),
                    serde_json::to_vec(&request).unwrap(),
                )
                .unwrap();
                let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "persistence::repository::linux::tests::killed_helpers_leave_complete_crash_window_files_release_locks_and_reconcile_once",
                        "--nocapture",
                    ])
                    .env_clear()
                    .env("CYOA_TEST_CRASH_ROOT", root.path())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .unwrap();
                let deadline = Instant::now() + Duration::from_secs(5);
                while !root.path().join("ready").exists() {
                    if child.try_wait().unwrap().is_some() {
                        panic!("crash child exited before barrier {point:?}");
                    }
                    if Instant::now() >= deadline {
                        child.kill().unwrap();
                        child.wait().unwrap();
                        panic!("crash barrier timeout {point:?}");
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
                assert_eq!(
                    repo.load(pending.target(), SaveCopy::Primary, &token)
                        .unwrap_err()
                        .kind,
                    StorageFailureKind::Busy
                );
                let pid = rustix::process::Pid::from_raw(child.id() as i32).unwrap();
                child.kill().unwrap();
                child.wait().unwrap();
                assert_eq!(
                    rustix::process::test_kill_process(pid),
                    Err(rustix::io::Errno::SRCH)
                );
                let dir = root.path().join("data/saves");
                let primary = dir.join(name(pending.target(), SaveCopy::Primary));
                let backup = dir.join(name(pending.target(), SaveCopy::Backup));
                let replaced = matches!(point, Point::FinalSync | Point::Cleanup);
                if replaced {
                    assert_eq!(std::fs::read(&primary).unwrap(), pending.bytes());
                } else if let Some(old) = &old {
                    assert_eq!(std::fs::read(&primary).unwrap(), *old);
                } else {
                    assert!(!primary.exists());
                }
                if let Some(old) = &old {
                    assert_eq!(
                        std::fs::read(&backup).unwrap(),
                        if point == Point::NewSync {
                            prior_backup.unwrap()
                        } else {
                            old.clone()
                        }
                    );
                } else {
                    assert!(!backup.exists());
                }
                let mut fresh = LocalRepository::new(root.path().join("data")).unwrap();
                let receipt = fresh.reconcile(pending.clone(), &PreparedWriteEvidence::default(), &token).unwrap();
                assert_eq!(receipt.stamp, pending.intended_stamp());
                let again = fresh.reconcile(pending.clone(), &PreparedWriteEvidence::default(), &token).unwrap();
                assert_eq!(receipt, again);
                assert_eq!(
                    fresh
                        .list(SavePage::new(None, 100).unwrap(), &token)
                        .unwrap()
                        .entries
                        .len(),
                    1
                );
                if let Some(old) = old {
                    assert_eq!(std::fs::read(&backup).unwrap(), old);
                } else {
                    assert!(!backup.exists());
                }
            }
        }
    }
    #[test]
    fn default_data_location_honors_absolute_xdg_and_rejects_relative_locations() {
        if let Some(expected) = std::env::var_os("CYOA_TEST_DATA_EXPECTED") {
            if expected == "reject" {
                assert!(data_directory(None).is_err());
            } else {
                assert_eq!(data_directory(None).unwrap(), PathBuf::from(expected));
            }
            return;
        }
        assert!(data_directory(Some("relative".into())).is_err());
        let root = tempfile::tempdir().unwrap();
        assert_eq!(
            data_directory(Some(root.path().into())).unwrap(),
            root.path()
        );
        for relative in [false, true] {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap());
            child.args(["--exact","persistence::repository::linux::tests::default_data_location_honors_absolute_xdg_and_rejects_relative_locations"]).env_clear();
            if relative {
                child
                    .env("XDG_DATA_HOME", "relative")
                    .env("CYOA_TEST_DATA_EXPECTED", "reject");
            } else {
                child
                    .env("XDG_DATA_HOME", root.path())
                    .env("HOME", root.path())
                    .env("CYOA_TEST_DATA_EXPECTED", root.path().join("cyoa"));
            }
            assert!(child.status().unwrap().success());
        }
        assert!(!root.path().join("cyoa").exists());
    }
    fn temp_in(dir: &Path, bytes: &[u8]) -> Option<tempfile::NamedTempFile> {
        let mut file = tempfile::Builder::new()
            .prefix(".cyoa-")
            .tempfile_in(dir)
            .unwrap();
        file.write_all(bytes).unwrap();
        Some(file)
    }
    fn links(path: &Path) -> u64 {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(path).unwrap().nlink()
    }
    #[test]
    fn unsupported_noreplace_publishes_by_rename_with_exactly_one_link() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("slot.json");
        let mut temp = temp_in(dir.path(), b"new");
        let temp_name = temp.as_ref().unwrap().path().to_owned();
        persist_with(&mut temp, &target, true, |_, _| {
            Err(io::Error::from_raw_os_error(rustix::io::Errno::INVAL.raw_os_error()))
        })
        .unwrap();
        assert!(temp.is_none());
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        assert_eq!(links(&target), 1, "a hard-link fallback would leave two");
        assert!(!temp_name.exists());
    }
    #[test]
    fn occupied_noreplace_target_is_never_replaced_and_the_temp_is_retained() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("slot.json");
        std::fs::write(&target, b"existing").unwrap();
        let mut temp = temp_in(dir.path(), b"new");
        let error = persist_with(&mut temp, &target, true, rename_noreplace).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert!(temp.as_ref().unwrap().path().exists(), "temp kept for cleanup");
        assert_eq!(std::fs::read(&target).unwrap(), b"existing");
        assert_eq!(links(&target), 1);
    }
    #[test]
    fn noreplace_publish_moves_the_temp_without_leaving_its_name() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("slot.json");
        let mut temp = temp_in(dir.path(), b"new");
        let temp_name = temp.as_ref().unwrap().path().to_owned();
        persist_with(&mut temp, &target, true, rename_noreplace).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        assert_eq!(links(&target), 1);
        assert!(!temp_name.exists());
    }
    #[test]
    fn writes_sweep_only_their_own_slots_orphaned_temps_under_the_lock() {
        let root = tempfile::tempdir().unwrap();
        let token = CancellationSource::default().token();
        let mut repo = LocalRepository::new(root.path().into()).unwrap();
        let receipt = repo.create(snapshot(), &PreparedWriteEvidence::default(), &token).unwrap();
        let other = repo.create(snapshot(), &PreparedWriteEvidence::default(), &token).unwrap();
        let saves = root.path().join("saves");
        let own = saves.join(format!("{}abc123", temp_prefix(&receipt.metadata.id)));
        let foreign = saves.join(format!("{}def456", temp_prefix(&other.metadata.id)));
        let unrelated = saves.join(".cyoa-unrelated");
        for path in [&own, &foreign, &unrelated] {
            std::fs::write(path, b"left behind by a killed helper").unwrap();
        }
        repo.replace(
            SaveTarget {
                id: receipt.metadata.id.clone(),
                expected_stamp: receipt.stamp,
            },
            snapshot(),
            &PreparedWriteEvidence::default(),
            &token,
        )
        .unwrap();
        assert!(!own.exists(), "this slot's orphan is swept");
        assert!(foreign.exists(), "another slot's temp may be a live write");
        assert!(unrelated.exists(), "unrecognized names are never removed");
    }
    #[test]
    fn writes_publish_the_prepared_attempt_before_their_first_disk_mutation() {
        // The first mutating step fails, so nothing reached disk; the caller's
        // sink must already hold the exact attempt the port contract promises.
        for operation in [StorageOperation::Create, StorageOperation::Replace] {
            let root = tempfile::tempdir().unwrap();
            let token = CancellationSource::default().token();
            let mut repo = LocalRepository::new(root.path().into()).unwrap();
            let existing = repo
                .create(snapshot(), &PreparedWriteEvidence::default(), &token)
                .unwrap();
            repo.ops = ops(Some(Point::NewWrite)).0;
            let sink = PreparedWriteEvidence::default();
            let error = match operation {
                StorageOperation::Create => repo.create(snapshot(), &sink, &token),
                _ => repo.replace(
                    SaveTarget {
                        id: existing.metadata.id.clone(),
                        expected_stamp: existing.stamp,
                    },
                    snapshot(),
                    &sink,
                    &token,
                ),
            }
            .unwrap_err();
            assert_eq!(error.visibility, WriteVisibility::Unchanged, "{operation:?}");
            let published = sink.pending().expect("published before mutating");
            assert_eq!(error.pending.as_deref(), Some(&published), "{operation:?}");
        }
    }
    #[test]
    fn writes_rejected_at_admission_publish_nothing() {
        let root = tempfile::tempdir().unwrap();
        let mut repo = LocalRepository::new(root.path().into()).unwrap();
        let source = CancellationSource::default();
        source.cancel();
        let sink = PreparedWriteEvidence::default();
        let error = repo.create(snapshot(), &sink, &source.token()).unwrap_err();
        assert_eq!(error.kind, StorageFailureKind::Cancelled);
        assert!(sink.pending().is_none());
    }
}
