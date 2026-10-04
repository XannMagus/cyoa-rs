#[cfg(test)]
mod tests {
    use super::*;
    use cyoa_application::cancellation::CancellationSource;
    use std::sync::{Arc, Mutex};
    fn snapshot()->SaveSnapshot {
        let id=SaveId::new("harbour-0123456789abcdef0123456789abcdef").unwrap();
        codec::decode(include_bytes!("../../tests/fixtures/saves/v1-full-story.json"),&id,SaveCopy::Primary).unwrap().snapshot
    }
    struct Ops { fault:Option<Point>, seen:Arc<Mutex<Vec<Point>>>, random:Option<[u8;16]> }
    impl FileOps for Ops {
        fn check(&mut self,p:Point)->io::Result<()> {self.seen.lock().unwrap().push(p);if self.fault==Some(p){Err(io::Error::other(format!("injected {p:?}")))}else{Ok(())}}
        fn now(&mut self)->io::Result<SavedAt>{Ok(SavedAt::new(0,0).unwrap())}
        fn random(&mut self)->io::Result<[u8;16]>{self.random.ok_or_else(||io::Error::other("entropy unavailable"))}
    }
    fn ops(fault:Option<Point>)->(Box<dyn FileOps>,Arc<Mutex<Vec<Point>>>) {
        let seen=Arc::new(Mutex::new(vec![]));
        (Box::new(Ops{fault,seen:seen.clone(),random:Some([0;16])}),seen)
    }
    #[test]
    fn every_write_failure_preserves_complete_files_visibility_and_retry_material() {
        for point in [Point::Open,Point::NewWrite,Point::NewFlush,Point::NewSync,Point::BackupWrite,Point::BackupFlush,Point::BackupSync,Point::BackupPersist,Point::BackupDirSync,Point::Recheck,Point::PrimaryPersist,Point::FinalSync,Point::Cleanup] {
            let root=tempfile::tempdir().unwrap();let token=CancellationSource::default().token();
            let mut repo=LocalRepository::new(root.path().into()).unwrap();
            let receipt=repo.create(snapshot(),&token).unwrap();
            let primary=root.path().join("saves").join(name(&receipt.metadata.id,SaveCopy::Primary));
            let backup=root.path().join("saves").join(name(&receipt.metadata.id,SaveCopy::Backup));
            let old=std::fs::read(&primary).unwrap();
            let pending=repo.prepare_replace(&SaveTarget{id:receipt.metadata.id.clone(),expected_stamp:receipt.stamp},&snapshot(),&token).unwrap();
            repo.ops=ops(Some(point)).0;
            let error=repo.execute_pending(pending.clone(),&token,StorageOperation::Replace).unwrap_err();
            assert_eq!(error.pending.as_deref(),Some(&pending),"{point:?}");
            let after=std::fs::read(&primary).unwrap();
            if matches!(point,Point::FinalSync|Point::Cleanup) {
                assert_eq!(after,pending.bytes(),"{point:?}");
                assert_eq!(error.visibility,WriteVisibility::Replaced{stamp:pending.intended_stamp()});
                assert_eq!(std::fs::read(&backup).unwrap(),old);
            } else {assert_eq!(after,old,"{point:?}");assert_eq!(error.visibility,WriteVisibility::Unchanged);}
            if backup.exists(){assert_eq!(std::fs::read(&backup).unwrap(),old);}
            repo.ops=ops(None).0;
            let reconciled=repo.reconcile(pending.clone(),&token).unwrap();
            assert_eq!(reconciled.stamp,pending.intended_stamp());
            assert_eq!(std::fs::read(&backup).unwrap(),old);
            assert_eq!(std::fs::read(&primary).unwrap(),pending.bytes());
            assert!(!std::fs::read_dir(primary.parent().unwrap()).unwrap().any(|e|e.unwrap().file_name().to_string_lossy().starts_with(".cyoa-")));
        }
    }
    #[test]
    fn first_create_receipt_loss_reconciles_without_duplicate_slots_or_backup_rotation() {
        let root=tempfile::tempdir().unwrap();let token=CancellationSource::default().token();let mut repo=LocalRepository::new(root.path().into()).unwrap();
        let pending=repo.prepare_create(&snapshot(),&token).unwrap();repo.ops=ops(Some(Point::FinalSync)).0;
        let error=repo.execute_pending(pending.clone(),&token,StorageOperation::Create).unwrap_err();
        assert!(matches!(error.visibility,WriteVisibility::Replaced{..}));repo.ops=ops(None).0;
        let receipt=repo.reconcile(pending.clone(),&token).unwrap();assert_eq!(&receipt.metadata.id,pending.target());assert_eq!(receipt.metadata.revision.get(),1);
        assert!(!root.path().join("saves").join(name(pending.target(),SaveCopy::Backup)).exists());
        assert_eq!(repo.list(SavePage::new(None,100).unwrap(),&token).unwrap().entries.len(),1);
        std::fs::write(root.path().join("saves").join(name(pending.target(),SaveCopy::Primary)),b"changed").unwrap();
        assert_eq!(repo.reconcile(pending,&token).unwrap_err().kind,StorageFailureKind::Conflict);
    }
    #[test]
    fn sync_order_constant_clock_collision_entropy_and_revision_exhaustion_are_observable() {
        let root=tempfile::tempdir().unwrap();let token=CancellationSource::default().token();let mut repo=LocalRepository::new(root.path().into()).unwrap();let (o,seen)=ops(None);repo.ops=o;
        let first=repo.create(snapshot(),&token).unwrap();
        assert_eq!(repo.create(snapshot(),&token).unwrap_err().kind,StorageFailureKind::Conflict);
        seen.lock().unwrap().clear();
        let second=repo.replace(SaveTarget{id:first.metadata.id.clone(),expected_stamp:first.stamp},snapshot(),&token).unwrap();
        assert_eq!(first.metadata.saved_at,second.metadata.saved_at);assert_eq!(second.metadata.revision.get(),2);
        let trace=seen.lock().unwrap();let pos=|p|trace.iter().position(|v|*v==p).unwrap();
        assert!(pos(Point::NewSync)<pos(Point::BackupSync));assert!(pos(Point::BackupSync)<pos(Point::BackupPersist));assert!(pos(Point::BackupPersist)<pos(Point::BackupDirSync));assert!(pos(Point::BackupDirSync)<pos(Point::PrimaryPersist));assert!(pos(Point::PrimaryPersist)<pos(Point::FinalSync));drop(trace);
        repo.ops=Box::new(Ops{fault:None,seen:seen.clone(),random:None});
        assert_eq!(repo.create(snapshot(),&token).unwrap_err().stage,StorageStage::Prepare);
        let path=root.path().join("saves").join(name(&first.metadata.id,SaveCopy::Primary));
        let mut doc:serde_json::Value=serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();doc["revision"]=u64::MAX.into();let bytes=serde_json::to_vec(&doc).unwrap();std::fs::write(&path,&bytes).unwrap();
        assert_eq!(repo.replace(SaveTarget{id:first.metadata.id,expected_stamp:codec::stamp(&bytes)},snapshot(),&token).unwrap_err().kind,StorageFailureKind::Conflict);
    }
    #[test]
    fn preparation_read_encode_and_cleanup_failures_keep_the_initiating_cause() {
        let root=tempfile::tempdir().unwrap();let token=CancellationSource::default().token();let mut repo=LocalRepository::new(root.path().into()).unwrap();let receipt=repo.create(snapshot(),&token).unwrap();
        for point in [Point::Read,Point::Encode] {
            repo.ops=ops(Some(point)).0;
            let error=repo.replace(SaveTarget{id:receipt.metadata.id.clone(),expected_stamp:receipt.stamp},snapshot(),&token).unwrap_err();
            assert_eq!(error.visibility,WriteVisibility::Unchanged);assert!(error.pending.is_none());assert!(error.message.contains(&format!("{point:?}")));
        }
        let error=failure(StorageOperation::Replace,StorageStage::Write,StorageFailureKind::Io,"original");
        let result=combine_cleanup::<()>(Err(error),Err(io::Error::other("cleanup")),StorageOperation::Replace).unwrap_err();
        assert_eq!(&*result.message,"original");assert_eq!(&*result.cleanup_errors,["cleanup"]);
    }
}
