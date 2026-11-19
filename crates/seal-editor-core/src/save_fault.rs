/// Why a save could not land, in terms the editor can explain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveFault {
    DiskFull,
    Permission,
    PathTooLong,
    Locked,
    Other,
}

/// Banner and retry for one failed save. `attempt` starts at 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SavePlan {
    pub heading_key: &'static str,
    pub reason_key: &'static str,
    pub action_key: &'static str,
    pub retry_ms: Option<u64>,
}

pub fn classify_save(kind: std::io::ErrorKind, os_code: Option<i32>) -> SaveFault {
    match os_code {
        Some(112 | 39 | 28) => return SaveFault::DiskFull,
        Some(32 | 33 | 11) => return SaveFault::Locked,
        Some(206 | 36 | 63) => return SaveFault::PathTooLong,
        Some(5 | 13 | 1) => return SaveFault::Permission,
        _ => {}
    }
    match kind {
        std::io::ErrorKind::StorageFull => SaveFault::DiskFull,
        std::io::ErrorKind::PermissionDenied => SaveFault::Permission,
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => SaveFault::Locked,
        std::io::ErrorKind::InvalidFilename => SaveFault::PathTooLong,
        _ => SaveFault::Other,
    }
}

pub fn plan_save_failure(fault: SaveFault, attempt: u32) -> Option<SavePlan> {
    let (reason_key, elsewhere, retryable) = match fault {
        SaveFault::DiskFull => ("save-disk-full", true, true),
        SaveFault::Permission => ("save-permission", true, false),
        SaveFault::PathTooLong => ("save-path-too-long", true, false),
        SaveFault::Locked => ("save-locked", false, true),
        SaveFault::Other => return None,
    };
    Some(SavePlan {
        heading_key: "save-cannot",
        reason_key,
        action_key: if elsewhere { "save-elsewhere" } else { "" },
        retry_ms: if retryable { retry_delay_ms(attempt) } else { None },
    })
}

fn retry_delay_ms(attempt: u32) -> Option<u64> {
    if attempt >= 3 {
        None
    } else {
        Some(200u64 << attempt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_permission_path_and_lock_each_get_a_message_and_a_retry_rule() {
        assert_eq!(classify_save(std::io::ErrorKind::StorageFull, None), SaveFault::DiskFull);
        assert_eq!(classify_save(std::io::ErrorKind::Other, Some(112)), SaveFault::DiskFull);
        assert_eq!(
            classify_save(std::io::ErrorKind::PermissionDenied, None),
            SaveFault::Permission
        );
        assert_eq!(classify_save(std::io::ErrorKind::Other, Some(206)), SaveFault::PathTooLong);
        assert_eq!(classify_save(std::io::ErrorKind::Other, Some(36)), SaveFault::PathTooLong);
        assert_eq!(classify_save(std::io::ErrorKind::Other, Some(32)), SaveFault::Locked);
        let denied = plan_save_failure(SaveFault::Permission, 0).unwrap();
        assert_eq!(denied.heading_key, "save-cannot");
        assert_eq!(denied.action_key, "save-elsewhere");
        assert_eq!(denied.reason_key, "save-permission");
        assert!(denied.retry_ms.is_none());
        assert_eq!(plan_save_failure(SaveFault::Locked, 0).unwrap().retry_ms, Some(200));
        assert_eq!(plan_save_failure(SaveFault::DiskFull, 1).unwrap().retry_ms, Some(400));
        assert_eq!(plan_save_failure(SaveFault::Locked, 2).unwrap().retry_ms, Some(800));
        assert_eq!(plan_save_failure(SaveFault::DiskFull, 3).unwrap().retry_ms, None);
        assert_eq!(
            plan_save_failure(SaveFault::PathTooLong, 0).unwrap().action_key,
            "save-elsewhere"
        );
    }
}
